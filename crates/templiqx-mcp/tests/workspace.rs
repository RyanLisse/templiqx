use anyhow::{Result, ensure};
use serde_json::Value;
use std::{fs, path::Path};
use templiqx_mcp::{Operations, RenderDocumentInput};

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let from_path = entry.path();
        let to_path = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from_path, &to_path)?;
        } else {
            fs::copy(&from_path, &to_path)?;
        }
    }
    Ok(())
}

fn copy_crm3_package(root: &Path) -> Result<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    copy_dir(&repo.join("examples/crm3"), &root.join("crm3"))
}

fn merge_data(root: &Path) -> Result<Value> {
    let data = fs::read(root.join("crm3/evals/bli-62-output.json"))?;
    Ok(serde_json::from_slice::<Value>(&data)?["merge_data"].clone())
}

#[test]
fn mcp_render_without_workspace_uses_safe_default() -> Result<()> {
    let packages = tempfile::tempdir()?;
    copy_crm3_package(packages.path())?;
    let service = templiqx_local::compose(packages.path())?;

    let envelope = Operations::render_document(
        &service,
        &RenderDocumentInput {
            package: "crm3".into(),
            template: "templates/v5-contract-template.docx".into(),
            data: merge_data(packages.path())?,
            output: "mcp-default.docx".into(),
            workspace: None,
        },
    );

    ensure!(
        envelope.ok,
        "unexpected diagnostics: {:?}",
        envelope.diagnostics
    );
    ensure!(
        packages
            .path()
            .join(".templiqx-workspace/crm3/mcp-default.docx")
            .exists()
    );
    ensure!(!packages.path().join("crm3/mcp-default.docx").exists());
    Ok(())
}

#[test]
fn mcp_render_with_workspace_matches_cli_contract() -> Result<()> {
    let packages = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    copy_crm3_package(packages.path())?;
    let service = templiqx_local::compose(packages.path())?;

    let envelope = Operations::render_document(
        &service,
        &RenderDocumentInput {
            package: "crm3".into(),
            template: "templates/v5-contract-template.docx".into(),
            data: merge_data(packages.path())?,
            output: "mcp-explicit/rendered.docx".into(),
            workspace: Some(workspace.path().to_string_lossy().into_owned()),
        },
    );

    ensure!(
        envelope.ok,
        "unexpected diagnostics: {:?}",
        envelope.diagnostics
    );
    ensure!(envelope.result.as_ref().expect("result").artifact == "mcp-explicit/rendered.docx");
    ensure!(
        workspace
            .path()
            .join("crm3/mcp-explicit/rendered.docx")
            .exists()
    );
    ensure!(
        !packages
            .path()
            .join("crm3/mcp-explicit/rendered.docx")
            .exists()
    );
    Ok(())
}
