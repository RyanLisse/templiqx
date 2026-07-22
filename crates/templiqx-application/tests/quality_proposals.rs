use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use templiqx_contracts::{
    BinaryScorer, CandidateEvidence, EligibilityComparator, EligibilityRule, MetricAggregation,
    MetricUnit, ObjectiveDirection, QUALITY_MAX_CANDIDATE_SOURCE_BYTES, QUALITY_MAX_CANDIDATES,
    QUALITY_MAX_REQUEST_BYTES, QualityCandidateSubmission, QualityObjective, QualityPolicy,
    QualityProposalRequest, TQX_QUALITY_BINDING_MISMATCH, TQX_QUALITY_CANDIDATE_INVALID,
    TQX_QUALITY_EVIDENCE_INVALID, TQX_QUALITY_LIMIT_EXCEEDED, TQX_QUALITY_POLICY_INVALID,
    TrialEvidence, TrialOutcome, fingerprint, quality_fixture_set_fingerprint,
    quality_package_fingerprint, quality_policy_fingerprint, quality_report_fingerprint,
};

type EvidenceMutation = fn(&mut CandidateEvidence);
type ObservingService = templiqx_application::TempliqxService<
    ObservingPackageStore,
    templiqx_local::FilesystemArtifactWorkspace,
    templiqx_local::DeterministicFakeRuntime,
    templiqx_local::UnsupportedLegacyAdapter,
    templiqx_local::UnsupportedDocumentRenderer,
    templiqx_local::UnsupportedDocumentInspector,
>;

#[derive(Clone)]
struct ObservingPackageStore {
    inner: templiqx_local::FilesystemPackageStore,
    manifest_calls: Arc<AtomicUsize>,
    identity_calls: Arc<AtomicUsize>,
    fail_manifest: bool,
    drift_second_identity: bool,
}

impl templiqx_ports::PackageStore for ObservingPackageStore {
    fn discover(
        &self,
    ) -> Result<Vec<templiqx_contracts::PackageManifest>, templiqx_ports::PortError> {
        self.inner.discover()
    }

    fn manifest(
        &self,
        package: &str,
    ) -> Result<templiqx_contracts::PackageManifest, templiqx_ports::PortError> {
        self.manifest_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_manifest {
            Err(templiqx_ports::PortError::Io(
                "manifest snapshot unavailable".into(),
            ))
        } else {
            self.inner.manifest(package)
        }
    }

    fn contract(
        &self,
        package: &str,
        contract: &str,
    ) -> Result<templiqx_contracts::Contract, templiqx_ports::PortError> {
        self.inner.contract(package, contract)
    }

    fn contract_source(
        &self,
        package: &str,
        contract: &str,
    ) -> Result<String, templiqx_ports::PortError> {
        self.inner.contract_source(package, contract)
    }

    fn package_identity(
        &self,
        package: &str,
    ) -> Result<templiqx_contracts::PackageIdentity, templiqx_ports::PortError> {
        let call = self.identity_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let mut identity = self.inner.package_identity(package)?;
        if self.drift_second_identity && call == 2 {
            identity.manifest.version = "9.9.9-drift".into();
        }
        Ok(identity)
    }

    fn artifact_bytes(
        &self,
        package: &str,
        relative_path: &str,
    ) -> Result<Vec<u8>, templiqx_ports::PortError> {
        self.inner.artifact_bytes(package, relative_path)
    }

    fn resolve_artifact_path(
        &self,
        package: &str,
        relative_path: &str,
    ) -> Result<PathBuf, templiqx_ports::PortError> {
        self.inner.resolve_artifact_path(package, relative_path)
    }

    fn relative_artifact_path(
        &self,
        package: &str,
        path: &Path,
    ) -> Result<String, templiqx_ports::PortError> {
        self.inner.relative_artifact_path(package, path)
    }

    fn put_contract(
        &self,
        _package: &str,
        _contract: &str,
        _source: &str,
        _expected_fingerprint: Option<&str>,
    ) -> Result<String, templiqx_ports::PortError> {
        Err(templiqx_ports::PortError::Unsupported(
            "test store is read-only".into(),
        ))
    }

    fn create_package(
        &self,
        _name: &str,
        _version: &str,
    ) -> Result<templiqx_contracts::PackageManifest, templiqx_ports::PortError> {
        Err(templiqx_ports::PortError::Unsupported(
            "test store is read-only".into(),
        ))
    }

    fn update_package(
        &self,
        _package: &str,
        _version: Option<&str>,
        _description: Option<&str>,
        _expected_fingerprint: &str,
    ) -> Result<templiqx_contracts::PackageManifest, templiqx_ports::PortError> {
        Err(templiqx_ports::PortError::Unsupported(
            "test store is read-only".into(),
        ))
    }

    fn delete_package(
        &self,
        _package: &str,
        _expected_fingerprint: &str,
    ) -> Result<String, templiqx_ports::PortError> {
        Err(templiqx_ports::PortError::Unsupported(
            "test store is read-only".into(),
        ))
    }

    fn attach_package_signature(
        &self,
        _package: &str,
        _signature: templiqx_contracts::PackageSignature,
        _expected_fingerprint: &str,
        _expected_identity_fingerprint: &str,
    ) -> Result<templiqx_contracts::PackageManifest, templiqx_ports::PortError> {
        Err(templiqx_ports::PortError::Unsupported(
            "test store is read-only".into(),
        ))
    }

    fn delete_contract(
        &self,
        _package: &str,
        _contract: &str,
        _expected_fingerprint: &str,
    ) -> Result<String, templiqx_ports::PortError> {
        Err(templiqx_ports::PortError::Unsupported(
            "test store is read-only".into(),
        ))
    }
}

fn observing_service(store: ObservingPackageStore, workspace: &Path) -> ObservingService {
    templiqx_application::TempliqxService::new(
        store,
        templiqx_local::FilesystemArtifactWorkspace::new(workspace).expect("workspace"),
        templiqx_local::DeterministicFakeRuntime,
        templiqx_local::UnsupportedLegacyAdapter,
        templiqx_local::UnsupportedDocumentRenderer,
        templiqx_local::UnsupportedDocumentInspector,
    )
}

fn packages_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .canonicalize()
        .expect("examples/packages root")
}

fn source() -> String {
    std::fs::read_to_string(packages_root().join("demo/contracts/greeting.yaml"))
        .expect("demo source")
}

fn policy() -> QualityPolicy {
    let scorer_fingerprint = "c".repeat(64);
    QualityPolicy {
        id: "quality-policy".into(),
        replicates_per_fixture: 1,
        minimum_semantic_cases: 1,
        maximum_infrastructure_failure_ppm: 0,
        claimed_evaluator_profile_fingerprint: "a".repeat(64),
        claimed_model_profile_fingerprint: "b".repeat(64),
        binary_scorers: vec![BinaryScorer {
            id: "correctness".into(),
            metric_id: "correctness_ratio".into(),
            claimed_scorer_fingerprint: scorer_fingerprint,
        }],
        objectives: vec![QualityObjective {
            id: "correctness".into(),
            metric_id: "correctness_ratio".into(),
            unit: MetricUnit::RatioPpm,
            aggregation: MetricAggregation::BinaryRatioPpm,
            direction: ObjectiveDirection::Maximize,
            claimed_measurement_profile_fingerprint: "d".repeat(64),
            currency_code: None,
            token_kind: None,
        }],
        eligibility_rules: vec![EligibilityRule {
            id: "correctness-floor".into(),
            metric_id: "correctness_ratio".into(),
            comparator: EligibilityComparator::Gte,
            unit: MetricUnit::RatioPpm,
            threshold: 850_000,
        }],
    }
}

fn service() -> (tempfile::TempDir, templiqx_local::LocalService) {
    let workspace = tempfile::tempdir().expect("workspace");
    let service = templiqx_local::compose_with_workspace(packages_root(), workspace.path())
        .expect("local service");
    (workspace, service)
}

fn service_with_contract(
    contract_source: &str,
) -> (
    tempfile::TempDir,
    tempfile::TempDir,
    templiqx_local::LocalService,
) {
    let packages = tempfile::tempdir().expect("packages");
    let demo = packages.path().join("demo");
    std::fs::create_dir_all(demo.join("contracts")).expect("contract directory");
    std::fs::create_dir_all(demo.join("translations")).expect("translation directory");
    std::fs::copy(
        packages_root().join("demo/templiqx.yaml"),
        demo.join("templiqx.yaml"),
    )
    .expect("manifest");
    for locale in ["en", "nl"] {
        std::fs::copy(
            packages_root().join(format!("demo/translations/{locale}.yaml")),
            demo.join(format!("translations/{locale}.yaml")),
        )
        .expect("translation");
    }
    std::fs::write(demo.join("contracts/greeting.yaml"), contract_source).expect("custom contract");
    let workspace = tempfile::tempdir().expect("workspace");
    let service = templiqx_local::compose_with_workspace(packages.path(), workspace.path())
        .expect("local service");
    (packages, workspace, service)
}

fn request(
    service: &templiqx_local::LocalService,
    candidate_source: String,
) -> QualityProposalRequest {
    request_for_base(service, &source(), candidate_source)
}

fn request_for_base(
    service: &templiqx_local::LocalService,
    base_source: &str,
    candidate_source: String,
) -> QualityProposalRequest {
    let base = templiqx_core::parse_contract(base_source, None).expect("base contract");
    let candidate =
        templiqx_core::parse_contract(&candidate_source, None).expect("candidate contract");
    let package_identity = service
        .export_package_identity("demo")
        .result
        .expect("package identity");
    let package_fingerprint =
        quality_package_fingerprint(&package_identity).expect("package fingerprint");
    let base_contract_fingerprint = fingerprint(&base).expect("base fingerprint");
    let fixture_set_fingerprint =
        quality_fixture_set_fingerprint(&base.evals).expect("fixture fingerprint");
    let policy = policy();
    let quality_policy_fingerprint =
        quality_policy_fingerprint(&policy).expect("policy fingerprint");
    let candidate_contract_fingerprint = fingerprint(&candidate).expect("candidate fingerprint");

    QualityProposalRequest {
        package: "demo".into(),
        contract_id: "greeting".into(),
        expected_package_fingerprint: package_fingerprint.clone(),
        expected_base_contract_fingerprint: base_contract_fingerprint.clone(),
        expected_fixture_set_fingerprint: fixture_set_fingerprint.clone(),
        policy,
        candidates: vec![QualityCandidateSubmission {
            candidate_source,
            synthetic_or_sanitized_data_attestation: true,
            evidence: CandidateEvidence {
                claimed_package_fingerprint: package_fingerprint,
                claimed_base_contract_fingerprint: base_contract_fingerprint,
                claimed_fixture_set_fingerprint: fixture_set_fingerprint,
                claimed_candidate_contract_fingerprint: candidate_contract_fingerprint,
                claimed_quality_policy_fingerprint: quality_policy_fingerprint,
                claimed_evaluator_profile_fingerprint: "a".repeat(64),
                claimed_model_profile_fingerprint: "b".repeat(64),
                claimed_scorer_fingerprints: BTreeMap::from([(
                    "correctness".into(),
                    "c".repeat(64),
                )]),
                claimed_measurement_profile_fingerprints: BTreeMap::from([(
                    "correctness_ratio".into(),
                    "d".repeat(64),
                )]),
                trials: vec![TrialEvidence {
                    fixture_id: "ryan".into(),
                    replicate_index: 0,
                    provider_attempt_count: 1,
                    outcome: TrialOutcome::Scored,
                    passed_scorers: vec!["correctness".into()],
                    failed_scorers: vec![],
                    observations: vec![],
                }],
            },
        }],
    }
}

fn set_candidate_source(submission: &mut QualityCandidateSubmission, candidate_source: String) {
    let candidate =
        templiqx_core::parse_contract(&candidate_source, None).expect("candidate contract");
    submission.evidence.claimed_candidate_contract_fingerprint =
        fingerprint(&candidate).expect("candidate fingerprint");
    submission.candidate_source = candidate_source;
}

fn candidate_source(index: usize) -> String {
    source().replace(
        "description: Produce a typed greeting.",
        &format!("description: Produce a typed greeting candidate {index}."),
    )
}

fn assert_candidate_has_diagnostic(
    service: &templiqx_local::LocalService,
    request: &QualityProposalRequest,
    code: &str,
    path: &str,
) {
    let envelope = service.assess_quality_proposals(request);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    let report = envelope.result.expect("quality report");
    let assessment = &report.candidate_assessments[0];
    assert!(!assessment.eligibility.eligible);
    assert!(
        assessment.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == code && diagnostic.json_pointer.as_deref() == Some(path)
        }),
        "missing ({code}, {path}) in {:?}",
        assessment.diagnostics
    );
}

fn assert_operation_diagnostic(
    service: &templiqx_local::LocalService,
    request: &QualityProposalRequest,
    code: &str,
    path: &str,
) {
    let envelope = service.assess_quality_proposals(request);
    assert!(!envelope.ok);
    assert!(envelope.result.is_none());
    assert!(envelope.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == code && diagnostic.json_pointer.as_deref() == Some(path)
    }));
}

#[test]
fn assessment_binds_current_artifacts_does_not_echo_source_and_does_not_mutate() {
    let (workspace, service) = service();
    let contract_path = packages_root().join("demo/contracts/greeting.yaml");
    let before_bytes = std::fs::read(&contract_path).expect("before source");
    let before_identity = service
        .export_package_identity("demo")
        .result
        .expect("before identity");
    let canary = "CUSTOMER-SECRET-7319";
    let candidate_source = source().replace(
        "description: Produce a typed greeting.",
        &format!("description: Produce a typed greeting. {canary}"),
    );
    let request = request(&service, candidate_source.clone());

    let envelope = service.assess_quality_proposals(&request);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    let report = envelope.result.expect("quality report");
    assert_eq!(
        report.computed_identities.package_fingerprint,
        request.expected_package_fingerprint
    );
    assert_eq!(
        report.computed_identities.base_contract_fingerprint,
        request.expected_base_contract_fingerprint
    );
    assert_eq!(
        report.computed_identities.fixture_set_fingerprint,
        request.expected_fixture_set_fingerprint
    );
    assert_eq!(
        report.candidate_assessments[0].proposal_change_paths,
        vec!["/description"]
    );
    let encoded = serde_json::to_string(&report).expect("report JSON");
    assert!(!encoded.contains(canary));
    assert!(!encoded.contains(&candidate_source));

    assert_eq!(
        std::fs::read(&contract_path).expect("after source"),
        before_bytes
    );
    assert_eq!(
        service
            .export_package_identity("demo")
            .result
            .expect("after identity"),
        before_identity
    );
    assert_eq!(
        std::fs::read_dir(workspace.path())
            .expect("workspace read")
            .count(),
        0,
        "proposal assessment must not create workspace artifacts"
    );
    assert_eq!(
        templiqx_application::CAPABILITY_CATALOG
            .iter()
            .filter(|operation| **operation == "assess_quality_proposals")
            .count(),
        1
    );
}

#[test]
fn stale_base_fails_closed_with_stable_diagnostic() {
    let (_workspace, service) = service();
    let mut request = request(&service, source());
    request.expected_base_contract_fingerprint = "0".repeat(64);

    let envelope = service.assess_quality_proposals(&request);
    assert!(!envelope.ok);
    assert!(envelope.result.is_none());
    assert_eq!(envelope.diagnostics.len(), 1);
    assert_eq!(
        envelope.diagnostics[0].code,
        templiqx_contracts::TQX_QUALITY_BASE_STALE
    );
}

#[test]
fn request_expectations_and_all_computed_identities_are_bound_exactly() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());

    let mut wrong_package = baseline.clone();
    wrong_package.expected_package_fingerprint = "0".repeat(64);
    assert_operation_diagnostic(
        &service,
        &wrong_package,
        TQX_QUALITY_BINDING_MISMATCH,
        "/expected_package_fingerprint",
    );

    let mut wrong_fixture_set = baseline.clone();
    wrong_fixture_set.expected_fixture_set_fingerprint = "0".repeat(64);
    assert_operation_diagnostic(
        &service,
        &wrong_fixture_set,
        TQX_QUALITY_BINDING_MISMATCH,
        "/expected_fixture_set_fingerprint",
    );

    let envelope = service.assess_quality_proposals(&baseline);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    let report = envelope.result.expect("quality report");
    assert_eq!(
        report.computed_identities.quality_policy_fingerprint,
        quality_policy_fingerprint(&baseline.policy).expect("policy fingerprint")
    );
    assert_eq!(
        report.report_fingerprint,
        quality_report_fingerprint(&report.payload()).expect("report fingerprint")
    );
    for (key, expected) in [
        (
            "package_identity",
            &report.computed_identities.package_fingerprint,
        ),
        (
            "base_contract",
            &report.computed_identities.base_contract_fingerprint,
        ),
        (
            "fixture_set",
            &report.computed_identities.fixture_set_fingerprint,
        ),
        (
            "quality_policy",
            &report.computed_identities.quality_policy_fingerprint,
        ),
        ("request", &report.computed_identities.request_fingerprint),
        ("report", &report.report_fingerprint),
    ] {
        assert_eq!(envelope.fingerprints.get(key), Some(expected));
    }
    assert_eq!(
        report.candidate_assessments[0]
            .candidate_fingerprint
            .as_ref(),
        Some(
            &baseline.candidates[0]
                .evidence
                .claimed_candidate_contract_fingerprint
        )
    );
}

#[test]
fn candidate_fixture_set_must_equal_the_current_base() {
    let (_workspace, service) = service();
    let changed_source = source().replace("greeting: Hello Ryan", "greeting: Different output");
    let request = request(&service, changed_source);

    let envelope = service.assess_quality_proposals(&request);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    let candidate = &envelope.result.expect("report").candidate_assessments[0];
    assert!(!candidate.eligibility.eligible);
    assert!(candidate.proposal_change_paths.contains(&"/evals".into()));
    assert!(
        candidate
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == templiqx_contracts::TQX_QUALITY_BINDING_MISMATCH)
    );
}

#[test]
fn every_computed_artifact_claim_is_bound_with_an_exact_path() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());
    let mutations: Vec<(EvidenceMutation, &str)> = vec![
        (
            |evidence| evidence.claimed_package_fingerprint = "0".repeat(64),
            "/candidates/0/evidence/claimed_package_fingerprint",
        ),
        (
            |evidence| evidence.claimed_base_contract_fingerprint = "0".repeat(64),
            "/candidates/0/evidence/claimed_base_contract_fingerprint",
        ),
        (
            |evidence| evidence.claimed_fixture_set_fingerprint = "0".repeat(64),
            "/candidates/0/evidence/claimed_fixture_set_fingerprint",
        ),
        (
            |evidence| evidence.claimed_candidate_contract_fingerprint = "0".repeat(64),
            "/candidates/0/evidence/claimed_candidate_contract_fingerprint",
        ),
        (
            |evidence| evidence.claimed_quality_policy_fingerprint = "0".repeat(64),
            "/candidates/0/evidence/claimed_quality_policy_fingerprint",
        ),
    ];

    for (mutate, path) in mutations {
        let mut request = baseline.clone();
        mutate(&mut request.candidates[0].evidence);
        assert_candidate_has_diagnostic(&service, &request, TQX_QUALITY_BINDING_MISMATCH, path);
    }
}

#[test]
fn every_opaque_profile_claim_is_checked_for_presence_value_and_extras() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());

    for (mutate, path) in [
        (
            (|evidence: &mut CandidateEvidence| {
                evidence.claimed_evaluator_profile_fingerprint = "0".repeat(64);
            }) as fn(&mut CandidateEvidence),
            "/evidence/claimed_evaluator_profile_fingerprint",
        ),
        (
            |evidence: &mut CandidateEvidence| {
                evidence.claimed_model_profile_fingerprint = "0".repeat(64);
            },
            "/evidence/claimed_model_profile_fingerprint",
        ),
        (
            |evidence: &mut CandidateEvidence| {
                evidence
                    .claimed_scorer_fingerprints
                    .insert("correctness".into(), "0".repeat(64));
            },
            "/evidence/claimed_scorer_fingerprints/correctness",
        ),
        (
            |evidence: &mut CandidateEvidence| {
                evidence.claimed_scorer_fingerprints.clear();
            },
            "/evidence/claimed_scorer_fingerprints/correctness",
        ),
        (
            |evidence: &mut CandidateEvidence| {
                evidence
                    .claimed_scorer_fingerprints
                    .insert("extra".into(), "e".repeat(64));
            },
            "/evidence/claimed_scorer_fingerprints",
        ),
        (
            |evidence: &mut CandidateEvidence| {
                evidence
                    .claimed_measurement_profile_fingerprints
                    .insert("correctness_ratio".into(), "0".repeat(64));
            },
            "/evidence/claimed_measurement_profile_fingerprints/correctness_ratio",
        ),
        (
            |evidence: &mut CandidateEvidence| {
                evidence.claimed_measurement_profile_fingerprints.clear();
            },
            "/evidence/claimed_measurement_profile_fingerprints/correctness_ratio",
        ),
        (
            |evidence: &mut CandidateEvidence| {
                evidence
                    .claimed_measurement_profile_fingerprints
                    .insert("extra".into(), "e".repeat(64));
            },
            "/evidence/claimed_measurement_profile_fingerprints",
        ),
    ] {
        let mut request = baseline.clone();
        mutate(&mut request.candidates[0].evidence);
        assert_candidate_has_diagnostic(&service, &request, TQX_QUALITY_EVIDENCE_INVALID, path);
    }
}

#[test]
fn malformed_fingerprint_shapes_permitted_by_deserialization_fail_closed() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());
    for malformed in ["a".repeat(63), "g".repeat(64)] {
        let mut malformed_package_expectation = baseline.clone();
        malformed_package_expectation.expected_package_fingerprint = malformed.clone();
        assert_operation_diagnostic(
            &service,
            &malformed_package_expectation,
            TQX_QUALITY_BINDING_MISMATCH,
            "/expected_package_fingerprint",
        );
        let mut malformed_base_expectation = baseline.clone();
        malformed_base_expectation.expected_base_contract_fingerprint = malformed.clone();
        assert_operation_diagnostic(
            &service,
            &malformed_base_expectation,
            templiqx_contracts::TQX_QUALITY_BASE_STALE,
            "/expected_base_contract_fingerprint",
        );
        let mut malformed_fixture_expectation = baseline.clone();
        malformed_fixture_expectation.expected_fixture_set_fingerprint = malformed.clone();
        assert_operation_diagnostic(
            &service,
            &malformed_fixture_expectation,
            TQX_QUALITY_BINDING_MISMATCH,
            "/expected_fixture_set_fingerprint",
        );

        for mutate in [
            (|evidence: &mut CandidateEvidence, value: String| {
                evidence.claimed_package_fingerprint = value;
            }) as fn(&mut CandidateEvidence, String),
            |evidence: &mut CandidateEvidence, value: String| {
                evidence.claimed_base_contract_fingerprint = value;
            },
            |evidence: &mut CandidateEvidence, value: String| {
                evidence.claimed_fixture_set_fingerprint = value;
            },
            |evidence: &mut CandidateEvidence, value: String| {
                evidence.claimed_candidate_contract_fingerprint = value;
            },
            |evidence: &mut CandidateEvidence, value: String| {
                evidence.claimed_quality_policy_fingerprint = value;
            },
        ] {
            let mut request = baseline.clone();
            mutate(&mut request.candidates[0].evidence, malformed.clone());
            let envelope = service.assess_quality_proposals(&request);
            assert!(envelope.ok, "{:?}", envelope.diagnostics);
            let diagnostics =
                &envelope.result.expect("report").candidate_assessments[0].diagnostics;
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == TQX_QUALITY_EVIDENCE_INVALID
                        || diagnostic.code == TQX_QUALITY_BINDING_MISMATCH
                }),
                "{diagnostics:?}"
            );
        }
        for mutate in [
            (|evidence: &mut CandidateEvidence, value: String| {
                evidence.claimed_evaluator_profile_fingerprint = value;
            }) as fn(&mut CandidateEvidence, String),
            |evidence: &mut CandidateEvidence, value: String| {
                evidence.claimed_model_profile_fingerprint = value;
            },
            |evidence: &mut CandidateEvidence, value: String| {
                evidence
                    .claimed_scorer_fingerprints
                    .insert("correctness".into(), value);
            },
            |evidence: &mut CandidateEvidence, value: String| {
                evidence
                    .claimed_measurement_profile_fingerprints
                    .insert("correctness_ratio".into(), value);
            },
        ] {
            let mut request = baseline.clone();
            mutate(&mut request.candidates[0].evidence, malformed.clone());
            let envelope = service.assess_quality_proposals(&request);
            assert!(envelope.ok, "{:?}", envelope.diagnostics);
            let diagnostics =
                &envelope.result.expect("report").candidate_assessments[0].diagnostics;
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == TQX_QUALITY_EVIDENCE_INVALID),
                "{diagnostics:?}"
            );
        }
        for mutate in [
            (|policy: &mut QualityPolicy, value: String| {
                policy.claimed_evaluator_profile_fingerprint = value;
            }) as fn(&mut QualityPolicy, String),
            |policy: &mut QualityPolicy, value: String| {
                policy.claimed_model_profile_fingerprint = value;
            },
            |policy: &mut QualityPolicy, value: String| {
                policy.binary_scorers[0].claimed_scorer_fingerprint = value;
            },
            |policy: &mut QualityPolicy, value: String| {
                policy.objectives[0].claimed_measurement_profile_fingerprint = value;
            },
        ] {
            let mut request = baseline.clone();
            mutate(&mut request.policy, malformed.clone());
            let envelope = service.assess_quality_proposals(&request);
            assert!(!envelope.ok);
            assert!(envelope.result.is_none());
            assert!(
                envelope
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == TQX_QUALITY_POLICY_INVALID),
                "{:?}",
                envelope.diagnostics
            );
        }
    }
}

#[test]
fn normalized_request_identity_excludes_candidate_source_bodies() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());
    let mut whitespace_only_body_change = baseline.clone();
    whitespace_only_body_change.candidates[0]
        .candidate_source
        .push_str("\n\n    ");

    let first = service
        .assess_quality_proposals(&baseline)
        .result
        .expect("baseline report");
    let second = service
        .assess_quality_proposals(&whitespace_only_body_change)
        .result
        .expect("body-only report");
    assert_eq!(
        first.computed_identities.request_fingerprint,
        second.computed_identities.request_fingerprint
    );
    assert_eq!(first.report_fingerprint, second.report_fingerprint);
    assert_eq!(first, second);
}

#[test]
fn invalid_candidate_diagnostic_never_echoes_parser_canary() {
    let (_workspace, service) = service();
    let mut request = request(&service, source());
    request.candidates[0].candidate_source = "messages: [CUSTOMER-SECRET-7319".into();

    let envelope = service.assess_quality_proposals(&request);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    let encoded = serde_json::to_string(&envelope).expect("envelope JSON");
    assert!(!encoded.contains("CUSTOMER-SECRET-7319"));
    let candidate = &envelope.result.expect("report").candidate_assessments[0];
    assert!(candidate.candidate_fingerprint.is_none());
    assert!(!candidate.eligibility.eligible);
    assert_eq!(
        candidate.diagnostics[0].code,
        templiqx_contracts::TQX_QUALITY_CANDIDATE_INVALID
    );
}

#[test]
fn json_yaml_and_contract_validation_failures_are_redacted_without_source_or_provider_echo() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());
    let invalid_sources = vec![
        (
            "{\"messages\":[\"JSON-CUSTOMER-SECRET-7319\"".to_owned(),
            "JSON-CUSTOMER-SECRET-7319",
        ),
        (
            "messages: [YAML-CUSTOMER-SECRET-7319".to_owned(),
            "YAML-CUSTOMER-SECRET-7319",
        ),
        (
            "provider_body: PROVIDER-BODY-SECRET-7319".to_owned(),
            "PROVIDER-BODY-SECRET-7319",
        ),
        (
            source().replace(
                "id: greeting",
                "id: invalid id VALIDATION-CUSTOMER-SECRET-7319",
            ),
            "VALIDATION-CUSTOMER-SECRET-7319",
        ),
    ];

    for (invalid_source, canary) in invalid_sources {
        let mut request = baseline.clone();
        request.candidates[0].candidate_source = invalid_source.clone();
        let envelope = service.assess_quality_proposals(&request);
        assert!(envelope.ok, "{:?}", envelope.diagnostics);
        let encoded = serde_json::to_string(&envelope).expect("envelope JSON");
        assert!(!encoded.contains(canary));
        assert!(!encoded.contains(&invalid_source));
        let assessment = &envelope.result.expect("report").candidate_assessments[0];
        assert!(assessment.candidate_fingerprint.is_none());
        assert!(assessment.diagnostics.iter().all(|diagnostic| {
            diagnostic.code == TQX_QUALITY_CANDIDATE_INVALID && !diagnostic.message.contains(canary)
        }));
    }
}

#[test]
fn candidate_controlled_input_component_and_extension_keys_never_enter_diagnostic_paths() {
    let (_workspace, service) = service();
    let mut candidate = templiqx_core::parse_contract(&source(), None).expect("candidate");
    candidate.inputs.insert(
        "INPUT CUSTOMER SECRET 7319".into(),
        candidate.inputs["name"].clone(),
    );
    candidate.components.insert(
        "COMPONENT CUSTOMER SECRET 7319".into(),
        candidate.components["salutation"].clone(),
    );
    candidate.extensions.insert(
        "EXTENSION CUSTOMER SECRET 7319".into(),
        templiqx_contracts::ExtensionSpec {
            capability: "CAPABILITY CUSTOMER SECRET 7319".into(),
            schema: serde_json::json!({}),
            value: serde_json::json!({}),
        },
    );
    let candidate_source = serde_yaml_ng::to_string(&candidate).expect("candidate YAML");
    let mut request = request(&service, source());
    request.candidates[0].candidate_source = candidate_source;

    let envelope = service.assess_quality_proposals(&request);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    let report = envelope.result.expect("report");
    let assessment = &report.candidate_assessments[0];
    assert!(assessment.candidate_fingerprint.is_none());
    assert_eq!(assessment.diagnostics.len(), 1);
    assert_eq!(
        assessment.diagnostics[0].json_pointer.as_deref(),
        Some("/candidates/0/candidate_source")
    );
    let encoded = serde_json::to_string(&report).expect("report JSON");
    assert!(!encoded.contains("CUSTOMER SECRET 7319"));
}

#[test]
fn manifest_snapshot_failures_and_package_identity_drift_fail_closed() {
    let (_baseline_workspace, baseline_service) = service();
    let request = request(&baseline_service, source());
    let inner = templiqx_local::FilesystemPackageStore::new(packages_root()).expect("store");

    let manifest_calls = Arc::new(AtomicUsize::new(0));
    let identity_calls = Arc::new(AtomicUsize::new(0));
    let workspace = tempfile::tempdir().expect("workspace");
    let observing = observing_service(
        ObservingPackageStore {
            inner: inner.clone(),
            manifest_calls: Arc::clone(&manifest_calls),
            identity_calls: Arc::clone(&identity_calls),
            fail_manifest: false,
            drift_second_identity: false,
        },
        workspace.path(),
    );
    let envelope = observing.assess_quality_proposals(&request);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    assert_eq!(manifest_calls.load(Ordering::SeqCst), 1);
    assert_eq!(identity_calls.load(Ordering::SeqCst), 2);

    let workspace = tempfile::tempdir().expect("workspace");
    let failing = observing_service(
        ObservingPackageStore {
            inner: inner.clone(),
            manifest_calls: Arc::new(AtomicUsize::new(0)),
            identity_calls: Arc::new(AtomicUsize::new(0)),
            fail_manifest: true,
            drift_second_identity: false,
        },
        workspace.path(),
    );
    let envelope = failing.assess_quality_proposals(&request);
    assert!(!envelope.ok);
    assert!(envelope.result.is_none());
    assert_eq!(envelope.diagnostics.len(), 1);
    assert_eq!(envelope.diagnostics[0].code, "TQX_IO");
    assert_eq!(
        envelope.diagnostics[0].message,
        "the package snapshot could not be read"
    );
    assert_eq!(
        envelope.diagnostics[0].json_pointer.as_deref(),
        Some("/package_identity/manifest")
    );

    let workspace = tempfile::tempdir().expect("workspace");
    let drifting = observing_service(
        ObservingPackageStore {
            inner,
            manifest_calls: Arc::new(AtomicUsize::new(0)),
            identity_calls: Arc::new(AtomicUsize::new(0)),
            fail_manifest: false,
            drift_second_identity: true,
        },
        workspace.path(),
    );
    let envelope = drifting.assess_quality_proposals(&request);
    assert!(!envelope.ok);
    assert!(envelope.result.is_none());
    assert_eq!(envelope.diagnostics.len(), 1);
    assert_eq!(envelope.diagnostics[0].code, TQX_QUALITY_BINDING_MISMATCH);
    assert_eq!(
        envelope.diagnostics[0].json_pointer.as_deref(),
        Some("/package_identity")
    );
}

#[test]
fn fixture_input_pii_and_candidate_schema_bodies_are_absent_from_the_report() {
    let pii_canaries = [
        "ryan@example.invalid",
        "NL00-TQX-PII-0000000001",
        "PROVIDER-BODY-SECRET-7319",
    ];
    let mut contract = templiqx_core::parse_contract(&source(), None).expect("base contract");
    contract.evals[0].inputs.insert(
        "name".into(),
        serde_json::Value::String(pii_canaries.join(" | ")),
    );
    contract.evals[0].context.insert(
        "provider_body".into(),
        serde_json::Value::String(pii_canaries[2].into()),
    );
    let custom_source = serde_yaml_ng::to_string(&contract).expect("custom contract YAML");
    let (_packages, _workspace, service) = service_with_contract(&custom_source);
    let request = request_for_base(&service, &custom_source, custom_source.clone());

    let envelope = service.assess_quality_proposals(&request);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    let report = envelope.result.expect("quality report");
    let encoded = serde_json::to_string(&report).expect("report JSON");
    for canary in pii_canaries {
        assert!(!encoded.contains(canary));
    }
    for forbidden_field in [
        "\"candidate_source\"",
        "\"messages\"",
        "\"inputs\"",
        "\"context\"",
        "\"fake_output\"",
        "\"provider_body\"",
        "\"parser_excerpt\"",
    ] {
        assert!(!encoded.contains(forbidden_field), "{forbidden_field}");
    }
}

#[test]
fn claimed_and_computed_duplicate_candidate_identities_both_fail_closed() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());

    let mut duplicate_claim = baseline.clone();
    duplicate_claim
        .candidates
        .push(duplicate_claim.candidates[0].clone());
    assert_operation_diagnostic(
        &service,
        &duplicate_claim,
        TQX_QUALITY_EVIDENCE_INVALID,
        "/candidates",
    );

    let mut duplicate_computed = baseline.clone();
    let mut differently_claimed = duplicate_computed.candidates[0].clone();
    differently_claimed
        .evidence
        .claimed_candidate_contract_fingerprint = "f".repeat(64);
    duplicate_computed.candidates.push(differently_claimed);
    let envelope = service.assess_quality_proposals(&duplicate_computed);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    let report = envelope.result.expect("quality report");
    assert_eq!(report.candidate_assessments.len(), 2);
    assert!(report.candidate_assessments.iter().all(|assessment| {
        !assessment.eligibility.eligible
            && assessment.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == TQX_QUALITY_EVIDENCE_INVALID
                    && diagnostic.json_pointer.as_deref() == Some("/candidate_fingerprint")
            })
    }));
}

#[test]
fn source_and_candidate_count_limits_accept_max_and_reject_max_plus_one() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());

    let mut max_source = baseline.clone();
    max_source.candidates[0]
        .candidate_source
        .push_str(&" ".repeat(QUALITY_MAX_CANDIDATE_SOURCE_BYTES - source().len()));
    assert_eq!(
        max_source.candidates[0].candidate_source.len(),
        QUALITY_MAX_CANDIDATE_SOURCE_BYTES
    );
    let envelope = service.assess_quality_proposals(&max_source);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);

    let mut oversized_source = max_source;
    oversized_source.candidates[0].candidate_source.push(' ');
    assert_operation_diagnostic(
        &service,
        &oversized_source,
        TQX_QUALITY_LIMIT_EXCEEDED,
        "/candidates/0/candidate_source",
    );

    let template = baseline.candidates[0].clone();
    let mut max_candidates = baseline.clone();
    max_candidates.candidates = (0..QUALITY_MAX_CANDIDATES)
        .map(|index| {
            let mut submission = template.clone();
            set_candidate_source(&mut submission, candidate_source(index));
            submission
        })
        .collect();
    let envelope = service.assess_quality_proposals(&max_candidates);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    assert_eq!(
        envelope.result.expect("report").candidate_assessments.len(),
        QUALITY_MAX_CANDIDATES
    );

    let mut too_many = max_candidates;
    let mut extra = template;
    set_candidate_source(&mut extra, candidate_source(QUALITY_MAX_CANDIDATES));
    too_many.candidates.push(extra);
    assert_operation_diagnostic(
        &service,
        &too_many,
        TQX_QUALITY_LIMIT_EXCEEDED,
        "/candidates",
    );
}

#[test]
fn canonical_service_request_size_accepts_four_mib_and_rejects_the_next_byte() {
    let (_workspace, service) = service();
    let baseline = request(&service, source());
    let template = baseline.candidates[0].clone();
    let mut at_max = baseline;
    at_max.candidates = (0..9)
        .map(|index| {
            let mut submission = template.clone();
            set_candidate_source(&mut submission, candidate_source(index));
            submission
        })
        .collect();

    let mut remaining = QUALITY_MAX_REQUEST_BYTES
        .checked_sub(serde_json::to_vec(&at_max).expect("request JSON").len())
        .expect("baseline request must fit");
    for candidate in &mut at_max.candidates {
        let capacity = QUALITY_MAX_CANDIDATE_SOURCE_BYTES - candidate.candidate_source.len();
        let add = remaining.min(capacity);
        candidate.candidate_source.push_str(&" ".repeat(add));
        remaining -= add;
    }
    assert_eq!(remaining, 0, "nine bounded sources must span four MiB");
    assert_eq!(
        serde_json::to_vec(&at_max).expect("request JSON").len(),
        QUALITY_MAX_REQUEST_BYTES
    );
    let envelope = service.assess_quality_proposals(&at_max);
    assert!(envelope.ok, "{:?}", envelope.diagnostics);

    let mut over_max = at_max;
    over_max
        .candidates
        .iter_mut()
        .find(|candidate| candidate.candidate_source.len() < QUALITY_MAX_CANDIDATE_SOURCE_BYTES)
        .expect("remaining source capacity")
        .candidate_source
        .push(' ');
    assert_eq!(
        serde_json::to_vec(&over_max).expect("request JSON").len(),
        QUALITY_MAX_REQUEST_BYTES + 1
    );
    assert_operation_diagnostic(&service, &over_max, TQX_QUALITY_LIMIT_EXCEEDED, "/");
}

#[test]
fn parse_invalid_candidates_are_order_invariant_by_stable_claimed_identity() {
    let (_workspace, service) = service();
    let mut forward = request(&service, source());
    forward.candidates[0].candidate_source = "messages: [FIRST-CANARY".into();
    let mut second = forward.candidates[0].clone();
    second.candidate_source = "messages: [SECOND-CANARY".into();
    second.evidence.claimed_candidate_contract_fingerprint = "f".repeat(64);
    forward.candidates.push(second);
    let mut reverse = forward.clone();
    reverse.candidates.reverse();

    let forward = service.assess_quality_proposals(&forward);
    let reverse = service.assess_quality_proposals(&reverse);
    assert!(forward.ok, "{:?}", forward.diagnostics);
    assert!(reverse.ok, "{:?}", reverse.diagnostics);
    assert_eq!(forward.result, reverse.result);
    let encoded = serde_json::to_string(&forward).expect("envelope JSON");
    assert!(!encoded.contains("FIRST-CANARY"));
    assert!(!encoded.contains("SECOND-CANARY"));
}
