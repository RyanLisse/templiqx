use anyhow::{Result, ensure};
use templiqx_contracts::{OperationEnvelope, RenderRequest};

const PACKAGE: &str = "synthetic-opco";
const CROSS_OPCO_PACKAGES: &[&str] = &["basenet-legal", "finly-advice", "simplicate-workflow"];
const EXTRACTION: &str = "hr-record-extraction";
const VALIDATION: &str = "hr-record-validation";
const CAPABILITIES: &[&str] = &["structured_output"];

fn packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .canonicalize()
        .expect("examples/packages root")
}

/// Cross-opco reference packages share the same package-driven conformance
/// surface exercised in `cross_opco_packages.rs`.
fn cross_opco_packages_root() -> std::path::PathBuf {
    packages_root()
}

fn result<T>(envelope: OperationEnvelope<T>) -> Result<T> {
    ensure!(
        envelope.ok,
        "{} failed: {:?}",
        envelope.operation,
        envelope.diagnostics
    );
    envelope
        .result
        .ok_or_else(|| anyhow::anyhow!("{} returned no result", envelope.operation))
}

#[test]
fn synthetic_opco_package_is_discoverable() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(packages_root(), workspace.path())?;
    let discovered = result(service.discover_packages())?;
    ensure!(
        discovered
            .iter()
            .any(|manifest| manifest.package == PACKAGE),
        "expected {PACKAGE} in discover_packages"
    );
    Ok(())
}

#[test]
fn synthetic_opco_validate_compile_execute_are_deterministic() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(packages_root(), workspace.path())?;
    let capabilities = CAPABILITIES
        .iter()
        .map(|cap| (*cap).to_string())
        .collect::<Vec<_>>();

    let first = result(service.validate_package(PACKAGE))?;
    let second = result(service.validate_package(PACKAGE))?;
    ensure!(first == second, "validate_package must be deterministic");

    let eval_report = result(service.test_package(PACKAGE, &capabilities))?;
    ensure!(
        eval_report.passed == 2 && eval_report.failed == 0,
        "expected both contract evals to pass"
    );

    let request: RenderRequest = serde_json::from_value(serde_json::json!({
        "inputs": {
            "employee_text": "Employee ID E-1042 joined the Platform Engineering department."
        },
        "context": {
            "department_code": "ENG-PLAT"
        }
    }))?;
    ensure!(
        service
            .compile_contract(PACKAGE, EXTRACTION, &request, &capabilities)
            .ok
    );
    let extraction_receipt = result(service.execute_contract(
        PACKAGE,
        EXTRACTION,
        &request,
        &capabilities,
        Some(serde_json::json!({
            "employee_id": "E-1042",
            "department": "Platform Engineering"
        })),
        false,
    ))?;
    let repeat = result(service.execute_contract(
        PACKAGE,
        EXTRACTION,
        &request,
        &capabilities,
        Some(serde_json::json!({
            "employee_id": "E-1042",
            "department": "Platform Engineering"
        })),
        false,
    ))?;
    ensure!(
        extraction_receipt.request_fingerprint == repeat.request_fingerprint
            && extraction_receipt.output_fingerprint == repeat.output_fingerprint,
        "execution receipts must match across runs"
    );

    let validation_request: RenderRequest = serde_json::from_value(serde_json::json!({
        "inputs": {
            "employee_id": "E-1042",
            "department": "Platform Engineering"
        },
        "context": {
            "expected_department": "Platform Engineering"
        }
    }))?;
    ensure!(
        service
            .compile_contract(PACKAGE, VALIDATION, &validation_request, &capabilities)
            .ok
    );
    let validation_receipt = result(service.execute_contract(
        PACKAGE,
        VALIDATION,
        &validation_request,
        &capabilities,
        Some(serde_json::json!({
            "valid": true,
            "reason": "department matches expected onboarding target"
        })),
        false,
    ))?;
    ensure!(
        validation_receipt.output_schema_valid,
        "validation output must satisfy schema"
    );
    Ok(())
}

#[test]
fn cross_opco_packages_are_discoverable_from_shared_root() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let service =
        templiqx_local::compose_with_workspace(cross_opco_packages_root(), workspace.path())?;
    let discovered = result(service.discover_packages())?;
    for package in CROSS_OPCO_PACKAGES {
        ensure!(
            discovered
                .iter()
                .any(|manifest| manifest.package == *package),
            "expected {package} in discover_packages"
        );
    }
    Ok(())
}
