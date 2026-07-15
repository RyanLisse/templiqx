//! Application tests for host-supplied authorized merge context.

use anyhow::{Result, ensure};
use templiqx_application::{synthetic_authorized_context, validate_authorized_context};
use templiqx_contracts::{OperationEnvelope, RenderRequest};
use templiqx_local::compose_with_workspace;

fn packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .canonicalize()
        .expect("examples/packages root")
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
fn basenet_legal_requires_matching_authorized_context() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let service = compose_with_workspace(packages_root(), workspace.path())?;
    let manifest = result(service.discover_packages())?
        .into_iter()
        .find(|m| m.package == "basenet-legal")
        .ok_or_else(|| anyhow::anyhow!("basenet-legal manifest"))?;
    let mut request = RenderRequest {
        inputs: Default::default(),
        context: Default::default(),
    };
    request.context.insert(
        "_templiqx_authorized_merge".into(),
        serde_json::to_value(synthetic_authorized_context("SYN-LEGAL-SCOPE-001"))?,
    );
    validate_authorized_context(&manifest, &request).map_err(|diagnostics| {
        anyhow::anyhow!("authorized context validation failed: {diagnostics:?}")
    })?;
    Ok(())
}

#[test]
fn basenet_legal_rejects_missing_authorized_context() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let service = compose_with_workspace(packages_root(), workspace.path())?;
    let manifest = result(service.discover_packages())?
        .into_iter()
        .find(|m| m.package == "basenet-legal")
        .ok_or_else(|| anyhow::anyhow!("basenet-legal manifest"))?;
    let request = RenderRequest {
        inputs: Default::default(),
        context: Default::default(),
    };
    let error = validate_authorized_context(&manifest, &request).expect_err("missing context");
    ensure!(
        error
            .iter()
            .any(|d| d.code == "TQX_AUTHORIZED_CONTEXT_MISSING")
    );
    Ok(())
}

#[test]
fn cross_opco_packages_pass_inline_evals_with_authorized_context() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let service = compose_with_workspace(packages_root(), workspace.path())?;
    let capabilities = vec!["structured_output".into()];
    for package in ["basenet-legal", "finly-advice", "simplicate-workflow"] {
        ensure!(result(service.validate_package(package))?.len() == 2);
        let report = result(service.test_package(package, &capabilities))?;
        ensure!(
            report.failed == 0 && report.passed == 2,
            "{package}: {:?}",
            report.cases
        );
    }
    Ok(())
}
