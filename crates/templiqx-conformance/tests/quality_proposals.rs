use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use anyhow::{Context, Result, ensure};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use rmcp::{
    ServiceExt as _,
    model::{CallToolRequestParams, JsonObject},
};
use serde::Deserialize;
use serde_json::{Value, json};
use templiqx_contracts::{
    CandidateAssessment, CandidateEvidence, InfrastructureFailureReason, MetricObservation,
    MetricUnit, QualityCandidateSubmission, QualityPolicy, QualityProposalRequest, TrialEvidence,
    TrialOutcome, fingerprint, quality_fixture_set_fingerprint, quality_package_fingerprint,
    quality_policy_fingerprint,
};
use templiqx_mcp::TempliqxMcp;
use tower::ServiceExt as _;

const FIXTURE: &str = include_str!("../../../examples/quality/reference-suite.json");
const SYNTHETIC_PII_CANARY: &str = "quality-canary@example.invalid NL00BANK0123456789";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceSuite {
    synthetic_pii_canary: String,
    policies: ReferencePolicies,
    evidence_cases: Vec<ReferenceEvidenceCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferencePolicies {
    high_consequence: QualityPolicy,
    general_advisory: QualityPolicy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceEvidenceCase {
    id: String,
    correctness_passes: usize,
    grounding_passes: usize,
    latency_ms: u64,
    #[serde(default)]
    latency_step_ms: u64,
    cost_microunits: u64,
    #[serde(default)]
    cost_step_microunits: u64,
    #[serde(default)]
    infrastructure_failures: usize,
    #[serde(default)]
    include_pii_canary: bool,
}

fn suite() -> ReferenceSuite {
    serde_json::from_str(FIXTURE).expect("quality reference suite")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else {
            std::fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

fn quality_root() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    copy_tree(
        &repo_root().join("examples/packages/demo"),
        &root.path().join("demo"),
    )?;
    let contract_path = root.path().join("demo/contracts/greeting.yaml");
    let source = std::fs::read_to_string(&contract_path)?;
    let source = source.replace(
        "      name: Ryan",
        &format!("      name: \"{SYNTHETIC_PII_CANARY}\""),
    );
    ensure!(
        source.contains(SYNTHETIC_PII_CANARY),
        "fixture-input canary insertion failed"
    );
    std::fs::write(contract_path, source)?;
    Ok(root)
}

fn candidate_source(base_source: &str, case: &ReferenceEvidenceCase, canary: &str) -> String {
    let suffix = if case.include_pii_canary {
        format!(" candidate {} {canary}", case.id)
    } else {
        format!(" candidate {}", case.id)
    };
    base_source.replace(
        "description: Produce a typed greeting.",
        &format!("description: Produce a typed greeting.{suffix}"),
    )
}

fn evidence(
    case: &ReferenceEvidenceCase,
    identities: &ComputedInputs,
    policy: &QualityPolicy,
    candidate_fingerprint: String,
) -> CandidateEvidence {
    let scorer_claims = policy
        .binary_scorers
        .iter()
        .map(|scorer| (scorer.id.clone(), scorer.claimed_scorer_fingerprint.clone()))
        .collect();
    let measurement_claims = policy
        .objectives
        .iter()
        .map(|objective| {
            (
                objective.metric_id.clone(),
                objective.claimed_measurement_profile_fingerprint.clone(),
            )
        })
        .collect();
    let mut semantic_index = 0;
    let trials = (0..policy.replicates_per_fixture)
        .map(|replicate_index| {
            let infrastructure = usize::from(replicate_index) < case.infrastructure_failures;
            let (outcome, passed_scorers, failed_scorers) = if infrastructure {
                (
                    TrialOutcome::InfrastructureFailure {
                        reason: InfrastructureFailureReason::Timeout,
                    },
                    Vec::new(),
                    Vec::new(),
                )
            } else {
                let mut passed = Vec::new();
                let mut failed = Vec::new();
                for (scorer, passes) in [
                    ("correctness", case.correctness_passes),
                    ("grounding", case.grounding_passes),
                ] {
                    if semantic_index < passes {
                        passed.push(scorer.to_owned());
                    } else {
                        failed.push(scorer.to_owned());
                    }
                }
                semantic_index += 1;
                (TrialOutcome::Scored, passed, failed)
            };
            TrialEvidence {
                fixture_id: "ryan".into(),
                replicate_index,
                provider_attempt_count: 1,
                outcome,
                passed_scorers,
                failed_scorers,
                observations: vec![
                    MetricObservation {
                        metric_id: "latency_ms".into(),
                        unit: MetricUnit::Milliseconds,
                        value: case.latency_ms + u64::from(replicate_index) * case.latency_step_ms,
                        claimed_measurement_profile_fingerprint: "7".repeat(64),
                        currency_code: None,
                        token_kind: None,
                    },
                    MetricObservation {
                        metric_id: "cost_microunits".into(),
                        unit: MetricUnit::CurrencyMicrounits,
                        value: case.cost_microunits
                            + u64::from(replicate_index) * case.cost_step_microunits,
                        claimed_measurement_profile_fingerprint: "8".repeat(64),
                        currency_code: Some("EUR".into()),
                        token_kind: None,
                    },
                ],
            }
        })
        .collect();

    CandidateEvidence {
        claimed_package_fingerprint: identities.package.clone(),
        claimed_base_contract_fingerprint: identities.base_contract.clone(),
        claimed_fixture_set_fingerprint: identities.fixture_set.clone(),
        claimed_candidate_contract_fingerprint: candidate_fingerprint,
        claimed_quality_policy_fingerprint: quality_policy_fingerprint(policy)
            .expect("policy fingerprint"),
        claimed_evaluator_profile_fingerprint: policy.claimed_evaluator_profile_fingerprint.clone(),
        claimed_model_profile_fingerprint: policy.claimed_model_profile_fingerprint.clone(),
        claimed_scorer_fingerprints: scorer_claims,
        claimed_measurement_profile_fingerprints: measurement_claims,
        trials,
    }
}

struct ComputedInputs {
    package: String,
    base_contract: String,
    fixture_set: String,
}

fn build_request(
    root: &Path,
    policy: QualityPolicy,
    cases: &[ReferenceEvidenceCase],
    canary: &str,
) -> Result<(QualityProposalRequest, BTreeMap<String, String>)> {
    let service = templiqx_local::compose(root)?;
    let base_source = std::fs::read_to_string(root.join("demo/contracts/greeting.yaml"))?;
    let base: templiqx_contracts::Contract =
        serde_yaml_ng::from_str(&base_source).context("base contract")?;
    let package_identity = service
        .export_package_identity("demo")
        .result
        .context("package identity")?;
    let identities = ComputedInputs {
        package: quality_package_fingerprint(&package_identity)?,
        base_contract: fingerprint(&base)?,
        fixture_set: quality_fixture_set_fingerprint(&base.evals)?,
    };
    let mut candidate_fingerprints = BTreeMap::new();
    let candidates = cases
        .iter()
        .map(|case| {
            let source = candidate_source(&base_source, case, canary);
            let parsed: templiqx_contracts::Contract =
                serde_yaml_ng::from_str(&source).context("candidate contract")?;
            let candidate_fingerprint = fingerprint(&parsed)?;
            candidate_fingerprints.insert(case.id.clone(), candidate_fingerprint.clone());
            Ok(QualityCandidateSubmission {
                candidate_source: source,
                synthetic_or_sanitized_data_attestation: true,
                evidence: evidence(case, &identities, &policy, candidate_fingerprint),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((
        QualityProposalRequest {
            package: "demo".into(),
            contract_id: "greeting".into(),
            expected_package_fingerprint: identities.package,
            expected_base_contract_fingerprint: identities.base_contract,
            expected_fixture_set_fingerprint: identities.fixture_set,
            policy,
            candidates,
        },
        candidate_fingerprints,
    ))
}

fn cases(suite: &ReferenceSuite, ids: &[&str]) -> Vec<ReferenceEvidenceCase> {
    ids.iter()
        .map(|id| {
            suite
                .evidence_cases
                .iter()
                .find(|case| case.id == *id)
                .unwrap_or_else(|| panic!("missing evidence case {id}"))
                .clone()
        })
        .collect()
}

fn assessment<'a>(
    assessments: &'a [CandidateAssessment],
    fingerprint: &str,
) -> &'a CandidateAssessment {
    assessments
        .iter()
        .find(|assessment| assessment.candidate_fingerprint.as_deref() == Some(fingerprint))
        .expect("candidate assessment")
}

#[test]
fn data_driven_950k_and_850k_floors_precede_pareto_ranking() -> Result<()> {
    let root = quality_root()?;
    let suite = suite();
    let high_cases = cases(
        &suite,
        &[
            "high_boundary",
            "high_quality",
            "high_dominated",
            "high_unsafe_cheap",
        ],
    );
    let (request, fingerprints) = build_request(
        root.path(),
        suite.policies.high_consequence.clone(),
        &high_cases,
        &suite.synthetic_pii_canary,
    )?;
    let report = templiqx_local::compose(root.path())?
        .assess_quality_proposals(&request)
        .result
        .context("high-consequence report")?;

    for id in ["high_boundary", "high_quality", "high_dominated"] {
        ensure!(
            assessment(&report.candidate_assessments, &fingerprints[id])
                .eligibility
                .eligible
        );
    }
    ensure!(
        !assessment(
            &report.candidate_assessments,
            &fingerprints["high_unsafe_cheap"]
        )
        .eligibility
        .eligible
    );
    let first_front: BTreeSet<_> = report
        .pareto_fronts
        .iter()
        .find(|front| front.rank == 1)
        .context("first Pareto front")?
        .candidate_fingerprints
        .iter()
        .cloned()
        .collect();
    ensure!(first_front.contains(&fingerprints["high_boundary"]));
    ensure!(first_front.contains(&fingerprints["high_quality"]));
    ensure!(!first_front.contains(&fingerprints["high_dominated"]));
    ensure!(!first_front.contains(&fingerprints["high_unsafe_cheap"]));

    let general_cases = cases(&suite, &["general_boundary", "general_under_floor"]);
    let (request, fingerprints) = build_request(
        root.path(),
        suite.policies.general_advisory.clone(),
        &general_cases,
        &suite.synthetic_pii_canary,
    )?;
    let report = templiqx_local::compose(root.path())?
        .assess_quality_proposals(&request)
        .result
        .context("general-advisory report")?;
    ensure!(
        assessment(
            &report.candidate_assessments,
            &fingerprints["general_boundary"]
        )
        .eligibility
        .eligible
    );
    ensure!(
        !assessment(
            &report.candidate_assessments,
            &fingerprints["general_under_floor"]
        )
        .eligibility
        .eligible
    );
    Ok(())
}

#[test]
fn stale_binding_infrastructure_exclusion_and_coverage_fail_closed() -> Result<()> {
    let root = quality_root()?;
    let suite = suite();
    let infrastructure = cases(&suite, &["infrastructure_excluded"]);

    let mut infrastructure_policy = suite.policies.high_consequence.clone();
    infrastructure_policy.minimum_semantic_cases = 19;
    let (mut stale, _) = build_request(
        root.path(),
        infrastructure_policy.clone(),
        &infrastructure,
        &suite.synthetic_pii_canary,
    )?;
    stale.expected_base_contract_fingerprint = "0".repeat(64);
    let stale = templiqx_local::compose(root.path())?.assess_quality_proposals(&stale);
    ensure!(!stale.ok && stale.result.is_none());
    ensure!(
        stale
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == templiqx_contracts::TQX_QUALITY_BASE_STALE)
    );

    let (request, fingerprints) = build_request(
        root.path(),
        infrastructure_policy,
        &infrastructure,
        &suite.synthetic_pii_canary,
    )?;
    let report = templiqx_local::compose(root.path())?
        .assess_quality_proposals(&request)
        .result
        .context("infrastructure report")?;
    let candidate = assessment(
        &report.candidate_assessments,
        &fingerprints["infrastructure_excluded"],
    );
    ensure!(!candidate.eligibility.eligible);
    ensure!(candidate.eligibility.semantic_trial_count == 19);
    ensure!(candidate.eligibility.infrastructure_trial_count == 1);
    ensure!(
        candidate
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code
                == templiqx_contracts::TQX_QUALITY_INFRASTRUCTURE_BUDGET)
    );
    let correctness = candidate
        .aggregates
        .iter()
        .find(|aggregate| aggregate.metric_id == "correctness_ratio")
        .context("correctness aggregate")?;
    ensure!(correctness.value == 1_000_000);

    let mut coverage_policy = suite.policies.high_consequence.clone();
    coverage_policy.maximum_infrastructure_failure_ppm = 1_000_000;
    let (request, fingerprints) = build_request(
        root.path(),
        coverage_policy,
        &infrastructure,
        &suite.synthetic_pii_canary,
    )?;
    let report = templiqx_local::compose(root.path())?
        .assess_quality_proposals(&request)
        .result
        .context("coverage report")?;
    let candidate = assessment(
        &report.candidate_assessments,
        &fingerprints["infrastructure_excluded"],
    );
    ensure!(!candidate.eligibility.eligible);
    ensure!(
        candidate
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code
                == templiqx_contracts::TQX_QUALITY_INSUFFICIENT_COVERAGE)
    );
    ensure!(
        !candidate
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code
                == templiqx_contracts::TQX_QUALITY_INFRASTRUCTURE_BUDGET)
    );
    Ok(())
}

#[test]
fn candidate_and_trial_order_are_invariant_and_pii_canary_is_not_echoed() -> Result<()> {
    let root = quality_root()?;
    let suite = suite();
    let selected = cases(&suite, &["high_boundary", "synthetic_pii_canary"]);
    let (forward, _) = build_request(
        root.path(),
        suite.policies.high_consequence.clone(),
        &selected,
        &suite.synthetic_pii_canary,
    )?;
    let mut reverse = forward.clone();
    reverse.candidates.reverse();
    for candidate in &mut reverse.candidates {
        candidate.evidence.trials.reverse();
        candidate.evidence.claimed_scorer_fingerprints = candidate
            .evidence
            .claimed_scorer_fingerprints
            .iter()
            .rev()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
    }
    let service = templiqx_local::compose(root.path())?;
    let forward = service.assess_quality_proposals(&forward);
    let reverse = service.assess_quality_proposals(&reverse);
    ensure!(forward.ok && reverse.ok);
    ensure!(forward.result == reverse.result);
    let serialized = serde_json::to_string(&forward)?;
    ensure!(!serialized.contains(&suite.synthetic_pii_canary));
    ensure!(!serialized.contains("quality-canary@example.invalid"));
    Ok(())
}

fn cli_envelope(root: &Path, request: &QualityProposalRequest) -> Result<Value> {
    static BUILT: OnceLock<()> = OnceLock::new();
    let repo = repo_root();
    if BUILT.get().is_none() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let status = Command::new(cargo)
            .current_dir(&repo)
            .args(["build", "--quiet", "-p", "templiqx-cli"])
            .status()?;
        ensure!(status.success(), "failed to build templiqx CLI");
        let _ = BUILT.set(());
    }
    let request_file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(request_file.as_file(), request)?;
    let binary = repo.join("target/debug").join(if cfg!(windows) {
        "templiqx.exe"
    } else {
        "templiqx"
    });
    let output = Command::new(binary)
        .arg("--root")
        .arg(root)
        .arg("--json")
        .arg("assess-quality-proposals")
        .arg("--request")
        .arg(request_file.path())
        .output()?;
    ensure!(
        output.status.success() || output.status.code() == Some(2),
        "CLI transport failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn arguments(value: Value) -> JsonObject {
    serde_json::from_value(value).expect("tool arguments object")
}

async fn mcp_envelope(root: &Path, request: &QualityProposalRequest) -> Result<Value> {
    let mcp_service = templiqx_local::compose(root)?;
    let (server_transport, client_transport) = tokio::io::duplex(1024 * 1024);
    let server_task = tokio::spawn(async move {
        let running = TempliqxMcp::new(mcp_service)
            .serve(server_transport)
            .await?;
        running.waiting().await?;
        anyhow::Ok(())
    });
    let client = ().serve(client_transport).await?;
    let mcp = client
        .call_tool(
            CallToolRequestParams::new("assess_quality_proposals".to_owned())
                .with_arguments(arguments(json!({"request": request.clone()}))),
        )
        .await?
        .structured_content
        .context("MCP structured content")?;
    client.cancel().await?;
    server_task.await??;
    Ok(mcp)
}

async fn http_envelope(
    root: &Path,
    request: &QualityProposalRequest,
) -> Result<(StatusCode, Value)> {
    let app = templiqx_http::router(templiqx_local::compose(root)?);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/operations/v1/packages/demo/quality/proposals:assess")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(request)?))?,
        )
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

async fn assert_surface_parity(
    root: &Path,
    name: &str,
    request: &QualityProposalRequest,
) -> Result<Value> {
    let rust = templiqx_local::compose(root)?.assess_quality_proposals(request);
    let rust_value = serde_json::to_value(&rust)?;
    let cli = cli_envelope(root, request)?;
    let mcp = mcp_envelope(root, request).await?;
    let (http_status, http) = http_envelope(root, request).await?;
    ensure!(rust_value == cli, "{name}: Rust/CLI envelope drift");
    ensure!(rust_value == mcp, "{name}: Rust/MCP envelope drift");
    ensure!(rust_value == http, "{name}: Rust/HTTP envelope drift");
    ensure!(
        http_status.is_success() == rust.ok,
        "{name}: HTTP status/envelope drift ({http_status})"
    );
    let encoded = serde_json::to_string(&rust_value)?;
    ensure!(
        !encoded.contains(SYNTHETIC_PII_CANARY),
        "{name}: fixture-input PII canary echoed"
    );
    for candidate in &request.candidates {
        ensure!(
            !encoded.contains(&candidate.candidate_source),
            "{name}: candidate source body echoed"
        );
    }
    Ok(rust_value)
}

#[tokio::test]
async fn reference_cases_match_across_rust_cli_mcp_and_http_without_echo() -> Result<()> {
    let root = quality_root()?;
    let suite = suite();
    ensure!(suite.synthetic_pii_canary == SYNTHETIC_PII_CANARY);

    let (happy, _) = build_request(
        root.path(),
        suite.policies.high_consequence.clone(),
        &cases(&suite, &["high_boundary"]),
        &suite.synthetic_pii_canary,
    )?;
    let happy_value = assert_surface_parity(root.path(), "happy", &happy).await?;
    ensure!(happy_value["ok"] == true);

    let (ineligible, _) = build_request(
        root.path(),
        suite.policies.high_consequence.clone(),
        &cases(&suite, &["high_unsafe_cheap"]),
        &suite.synthetic_pii_canary,
    )?;
    let ineligible_value = assert_surface_parity(root.path(), "ineligible", &ineligible).await?;
    ensure!(ineligible_value["ok"] == true);
    ensure!(
        ineligible_value["result"]["candidate_assessments"][0]["eligibility"]["eligible"] == false
    );

    let mut stale = happy.clone();
    stale.expected_base_contract_fingerprint = "0".repeat(64);
    let stale_value = assert_surface_parity(root.path(), "stale", &stale).await?;
    ensure!(stale_value["ok"] == false && stale_value["result"].is_null());

    let mut malformed = happy.clone();
    malformed.candidates[0].candidate_source = format!("messages: [{SYNTHETIC_PII_CANARY}");
    let malformed_value = assert_surface_parity(root.path(), "malformed", &malformed).await?;
    ensure!(malformed_value["ok"] == true);
    ensure!(
        malformed_value["result"]["candidate_assessments"][0]["candidate_fingerprint"].is_null()
    );
    ensure!(
        malformed_value["result"]["candidate_assessments"][0]["eligibility"]["eligible"] == false
    );

    let (all_infrastructure, _) = build_request(
        root.path(),
        suite.policies.high_consequence.clone(),
        &cases(&suite, &["all_infrastructure"]),
        &suite.synthetic_pii_canary,
    )?;
    let all_infrastructure_value =
        assert_surface_parity(root.path(), "all-infrastructure", &all_infrastructure).await?;
    ensure!(all_infrastructure_value["ok"] == true);
    let assessment = &all_infrastructure_value["result"]["candidate_assessments"][0];
    ensure!(assessment["eligibility"]["eligible"] == false);
    ensure!(assessment["eligibility"]["semantic_trial_count"] == 0);
    ensure!(assessment["eligibility"]["infrastructure_trial_count"] == 20);
    let trials = assessment["trial_summaries"]
        .as_array()
        .context("all-infrastructure trial summaries")?;
    let latency_values: BTreeSet<_> = trials
        .iter()
        .filter_map(|trial| {
            trial["observations"]
                .as_array()?
                .iter()
                .find_map(|observation| {
                    (observation["metric_id"] == "latency_ms")
                        .then(|| observation["value"].as_u64())
                        .flatten()
                })
        })
        .collect();
    let cost_values: BTreeSet<_> = trials
        .iter()
        .filter_map(|trial| {
            trial["observations"]
                .as_array()?
                .iter()
                .find_map(|observation| {
                    (observation["metric_id"] == "cost_microunits")
                        .then(|| observation["value"].as_u64())
                        .flatten()
                })
        })
        .collect();
    ensure!(latency_values.len() == 20 && cost_values.len() == 20);
    Ok(())
}
