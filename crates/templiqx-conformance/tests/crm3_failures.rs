use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use templiqx_application::TempliqxService;
use templiqx_contracts::{RenderRequest, fingerprint};
use templiqx_local::{
    FilesystemArtifactWorkspace, FilesystemPackageStore, UnsupportedDocumentRenderer,
    UnsupportedLegacyAdapter,
};
use templiqx_mock::{
    ReceiptPayloadPolicy, ScenarioManifest, ScriptedRuntime, failure_receipt_fingerprint,
    load_inventory, success_receipt_fingerprint,
};
use templiqx_ports::RuntimeFailureCode;

const PACKAGE: &str = "crm3";
const CONTRACT: &str = "bli-61-date-term-extraction";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn scenario_paths() -> Result<Vec<PathBuf>> {
    let root = repo_root().join("examples/crm3/scenarios");
    let inventory = load_inventory(root.join("inventory.json"), PACKAGE)?;
    Ok(inventory
        .scenarios
        .into_iter()
        .map(|entry| {
            root.parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(entry.manifest)
        })
        .collect())
}

fn outcome_fingerprint(
    envelope: &templiqx_contracts::OperationEnvelope<templiqx_contracts::ExecutionReceipt>,
) -> String {
    if let Some(receipt) = &envelope.result {
        return if envelope.ok {
            success_receipt_fingerprint(receipt)
        } else {
            failure_receipt_fingerprint(
                envelope
                    .diagnostics
                    .first()
                    .map_or("unknown", |d| d.code.as_str()),
                &receipt.output_fingerprint,
                None,
            )
        };
    }
    let diagnostic = envelope
        .diagnostics
        .first()
        .expect("failed outcome has diagnostic");
    let failure_fingerprint = diagnostic
        .help
        .as_deref()
        .and_then(|help| {
            help.split_whitespace()
                .find_map(|field| field.strip_prefix("fingerprint="))
        })
        .unwrap_or_default();
    let retry_after_ms = diagnostic.help.as_deref().and_then(|help| {
        help.split_whitespace()
            .find_map(|field| field.strip_prefix("retry_after_ms="))
            .and_then(|v| v.parse().ok())
    });
    failure_receipt_fingerprint(&diagnostic.code, failure_fingerprint, retry_after_ms)
}

fn read_request(path: &Path) -> Result<RenderRequest> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn service(
    runtime: ScriptedRuntime,
) -> Result<
    TempliqxService<
        FilesystemPackageStore,
        FilesystemArtifactWorkspace,
        ScriptedRuntime,
        UnsupportedLegacyAdapter,
        UnsupportedDocumentRenderer,
    >,
> {
    let workspace = tempfile::tempdir()?.keep();
    Ok(TempliqxService::new(
        FilesystemPackageStore::new(repo_root().join("examples"))?,
        FilesystemArtifactWorkspace::new(workspace)?,
        runtime,
        UnsupportedLegacyAdapter,
        UnsupportedDocumentRenderer,
    ))
}

fn assert_failure(code: RuntimeFailureCode) -> Result<()> {
    let runtime = ScriptedRuntime::failure(code.as_str(), code);
    let service = service(runtime.clone())?;
    let capabilities = ["structured_output".to_string()];
    let request = read_request(&repo_root().join("examples/crm3/evals/bli-61-request.json"))?;
    let envelope = service.execute_contract(PACKAGE, CONTRACT, &request, &capabilities, None);

    ensure!(!envelope.ok, "failure envelope unexpectedly ok");
    ensure!(envelope.result.is_none(), "failure fabricated receipt");
    ensure!(
        runtime.stats().attempts == 1,
        "runtime executed more than once"
    );
    ensure!(
        envelope
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == code.as_str()
                && diagnostic.help.as_deref().is_some_and(|help| {
                    help.contains("adapter=templiqx-scripted-runtime")
                        && help.contains("fingerprint=")
                })),
        "missing stable diagnostic {}: {:?}",
        code.as_str(),
        envelope.diagnostics
    );
    Ok(())
}

#[test]
fn runtime_failures_are_deterministic_and_payload_free() -> Result<()> {
    for code in [
        RuntimeFailureCode::Timeout,
        RuntimeFailureCode::RateLimited,
        RuntimeFailureCode::Unavailable,
        RuntimeFailureCode::InvalidResponse,
        RuntimeFailureCode::Permanent,
    ] {
        assert_failure(code)?;
    }
    Ok(())
}

#[test]
fn crm3_scenario_inventory_is_versioned_and_data_driven() -> Result<()> {
    let paths = scenario_paths()?;
    ensure!(
        paths.len() >= 6,
        "expected CRM3 happy, ambiguous, missing, invalid and drafting scenarios"
    );

    let mut ids = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for path in paths {
        let manifest = ScenarioManifest::load(&path)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        ensure!(
            ids.insert(manifest.id.clone()),
            "duplicate scenario id {}",
            manifest.id
        );
        kinds.insert(format!("{:?}", manifest.kind));

        ensure!(
            manifest.golden_receipt_fingerprint.is_some(),
            "{} is missing a golden receipt fingerprint",
            manifest.id
        );
    }

    for required in ["HappyPath", "Ambiguous", "Missing", "Invalid", "Drafting"] {
        ensure!(kinds.contains(required), "missing {required} scenario");
    }
    Ok(())
}

#[test]
fn checked_in_crm3_scenarios_execute_without_sleeping() -> Result<()> {
    for path in scenario_paths()? {
        let manifest = ScenarioManifest::load(&path)?;
        let scenario_dir = path.parent().context("scenario manifest parent")?;
        let request_path = manifest
            .input
            .as_ref()
            .map(|input| scenario_dir.join(input))
            .context("scenario must declare an input")?;
        let expected_output = match &manifest.expected_output {
            Some(output) => Some(serde_json::from_slice(&fs::read(
                scenario_dir.join(output),
            )?)?),
            None => None,
        };

        if let (Some(output), Some(expected)) =
            (&expected_output, &manifest.expected_output_fingerprint)
        {
            ensure!(
                fingerprint(output)? == *expected,
                "expected output fingerprint drifted"
            );
        }

        let runtime = ScriptedRuntime::from_manifest(manifest.clone());
        let service = service(runtime.clone())?;
        let capabilities = ["structured_output".to_string()];
        let envelope = service.execute_contract(
            PACKAGE,
            &manifest.contract,
            &read_request(&request_path)?,
            &capabilities,
            expected_output,
        );

        ensure!(manifest.expected_status == if envelope.ok { "success" } else { "failure" });
        if let Some(expected_failure) = &manifest.expected_failure {
            ensure!(
                manifest
                    .expected_diagnostics
                    .iter()
                    .any(|code| code == expected_failure)
            );
        }

        for code in &manifest.expected_diagnostics {
            ensure!(
                envelope
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == *code),
                "{} did not emit expected diagnostic {code}: {:?}",
                manifest.id,
                envelope.diagnostics
            );
        }

        match manifest.receipt_payload_policy {
            ReceiptPayloadPolicy::FingerprintsOnly => {
                ensure!(
                    envelope.result.is_some(),
                    "{} should produce a receipt",
                    manifest.id
                );
                ensure!(
                    envelope.fingerprints.contains_key("request")
                        && envelope.fingerprints.contains_key("output"),
                    "{} should expose fingerprints",
                    manifest.id
                );
            }
            ReceiptPayloadPolicy::NoSuccessfulReceipt => {
                ensure!(
                    envelope.result.is_none(),
                    "{} should not produce a receipt",
                    manifest.id
                );
            }
        }

        ensure!(
            runtime.stats().attempts == 1,
            "{} should execute once",
            manifest.id
        );
        ensure!(
            runtime.stats().elapsed_ms <= 25,
            "{} used unexpected real or unbounded delay",
            manifest.id
        );
        let actual = outcome_fingerprint(&envelope);
        ensure!(
            manifest.golden_receipt_fingerprint.as_deref() == Some(actual.as_str()),
            "{} golden receipt drifted: expected {:?}, actual {}",
            manifest.id,
            manifest.golden_receipt_fingerprint,
            actual
        );
    }
    Ok(())
}

#[test]
fn host_retry_exhaustion_uses_exact_terminal_code_and_no_payload() -> Result<()> {
    let code = RuntimeFailureCode::HostRetryExhausted;
    let runtime = ScriptedRuntime::failure("host-retry-exhausted", code);
    let harness = service(runtime.clone())?;
    let capabilities = ["structured_output".to_string()];
    let request = read_request(&repo_root().join("examples/crm3/evals/bli-61-request.json"))?;
    let envelope = harness.execute_contract(PACKAGE, CONTRACT, &request, &capabilities, None);

    ensure!(!envelope.ok, "retry exhaustion envelope unexpectedly ok");
    ensure!(
        envelope.result.is_none(),
        "retry exhaustion must not fabricate a receipt"
    );
    ensure!(
        runtime.stats().attempts == 1,
        "retry exhaustion should execute once"
    );
    let diagnostic = envelope
        .diagnostics
        .iter()
        .find(|item| item.code == code.as_str())
        .context("missing TQX_HOST_RETRY_EXHAUSTED diagnostic")?;
    ensure!(
        diagnostic.help.as_deref().is_some_and(|help| {
            help.contains("adapter=templiqx-scripted-runtime") && help.contains("fingerprint=")
        }),
        "retry exhaustion diagnostic missing adapter metadata: {:?}",
        diagnostic
    );

    let runtime_repeat = ScriptedRuntime::failure("host-retry-exhausted", code);
    let service_repeat = service(runtime_repeat)?;
    let envelope_repeat =
        service_repeat.execute_contract(PACKAGE, CONTRACT, &request, &capabilities, None);
    ensure!(
        envelope_repeat.diagnostics == envelope.diagnostics,
        "retry exhaustion diagnostics must be deterministic"
    );
    Ok(())
}

#[test]
fn contract_source_mutation_changes_package_fingerprint() -> Result<()> {
    let examples = repo_root().join("examples");
    let workspace = tempfile::tempdir()?.keep();
    let service = TempliqxService::new(
        FilesystemPackageStore::new(examples.clone())?,
        FilesystemArtifactWorkspace::new(workspace)?,
        ScriptedRuntime::success(),
        UnsupportedLegacyAdapter,
        UnsupportedDocumentRenderer,
    );
    let baseline = service.validate_package(PACKAGE);
    ensure!(
        baseline.ok,
        "baseline package invalid: {:?}",
        baseline.diagnostics
    );
    let baseline_fingerprint = baseline
        .fingerprints
        .get("package")
        .context("package fingerprint")?
        .clone();

    let contract_path = examples
        .join(PACKAGE)
        .join("contracts")
        .join(format!("{CONTRACT}.yaml"));
    let source = fs::read_to_string(&contract_path)?;
    let baseline_contract_fingerprint = fingerprint(&source)?;
    let anchor = source
        .find("Dutch")
        .context("contract description anchor for mutation")?;
    let mut bytes = source.into_bytes();
    bytes[anchor] = b'X';
    let mutated = String::from_utf8(bytes)?;
    ensure!(
        baseline_contract_fingerprint != fingerprint(&mutated)?,
        "single-byte mutation did not change contract fingerprint"
    );

    let temp_packages = tempfile::tempdir()?;
    let temp_package = temp_packages.path().join(PACKAGE);
    copy_dir_all(&examples.join(PACKAGE), &temp_package)?;
    fs::write(
        temp_package
            .join("contracts")
            .join(format!("{CONTRACT}.yaml")),
        mutated,
    )?;
    let temp_workspace = tempfile::tempdir()?.keep();
    let mutated_service = TempliqxService::new(
        FilesystemPackageStore::new(temp_packages.path())?,
        FilesystemArtifactWorkspace::new(temp_workspace)?,
        ScriptedRuntime::success(),
        UnsupportedLegacyAdapter,
        UnsupportedDocumentRenderer,
    );
    let mutated_validation = mutated_service.validate_package(PACKAGE);
    ensure!(
        mutated_validation.ok,
        "mutated package invalid: {:?}",
        mutated_validation.diagnostics
    );
    let mutated_fingerprint = mutated_validation
        .fingerprints
        .get("package")
        .context("mutated package fingerprint")?;
    ensure!(
        baseline_fingerprint != *mutated_fingerprint,
        "single-byte contract mutation did not change package fingerprint"
    );
    Ok(())
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[test]
fn mock_scenario_parser_rejects_spec_gaps() -> Result<()> {
    let valid = r#"{
      "api_version": "templiqx.mock/v1alpha1",
      "id": "valid",
      "contract": "bli-61-date-term-extraction",
      "kind": "happy_path",
      "receipt_payload_policy": "fingerprints_only",
      "steps": [
        { "id": "request", "kind": "request_received" },
        { "id": "done", "kind": "runtime_success" }
      ]
    }"#;
    ensure!(ScenarioManifest::from_json_slice(valid.as_bytes()).is_ok());

    for (name, body) in [
        (
            "unknown field",
            r#"{
              "api_version": "templiqx.mock/v1alpha1",
              "id": "bad",
              "contract": "bli-61-date-term-extraction",
              "kind": "happy_path",
              "receipt_payload_policy": "fingerprints_only",
              "extra": true,
              "steps": [{ "id": "request", "kind": "request_received" }, { "id": "done", "kind": "runtime_success" }]
            }"#,
        ),
        (
            "empty steps",
            r#"{
              "api_version": "templiqx.mock/v1alpha1",
              "id": "bad",
              "contract": "bli-61-date-term-extraction",
              "kind": "happy_path",
              "receipt_payload_policy": "fingerprints_only",
              "steps": []
            }"#,
        ),
        (
            "zero delay",
            r#"{
              "api_version": "templiqx.mock/v1alpha1",
              "id": "bad",
              "contract": "bli-61-date-term-extraction",
              "kind": "happy_path",
              "receipt_payload_policy": "fingerprints_only",
              "steps": [
                { "id": "request", "kind": "request_received" },
                { "id": "wait", "kind": "delay", "delay_ms": 0 },
                { "id": "done", "kind": "runtime_success" }
              ]
            }"#,
        ),
        (
            "duplicate ids",
            r#"{
              "api_version": "templiqx.mock/v1alpha1",
              "id": "bad",
              "contract": "bli-61-date-term-extraction",
              "kind": "happy_path",
              "receipt_payload_policy": "fingerprints_only",
              "steps": [
                { "id": "same", "kind": "request_received" },
                { "id": "same", "kind": "runtime_success" }
              ]
            }"#,
        ),
        (
            "bad lifecycle",
            r#"{
              "api_version": "templiqx.mock/v1alpha1",
              "id": "bad",
              "contract": "bli-61-date-term-extraction",
              "kind": "happy_path",
              "receipt_payload_policy": "fingerprints_only",
              "steps": [
                { "id": "done", "kind": "runtime_success" },
                { "id": "request", "kind": "request_received" }
              ]
            }"#,
        ),
    ] {
        if ScenarioManifest::from_json_slice(body.as_bytes()).is_ok() {
            bail!("parser accepted {name}");
        }
    }

    Ok(())
}
