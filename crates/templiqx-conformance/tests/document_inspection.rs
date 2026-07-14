use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{fs, path::Path};
use templiqx_application::InspectDocumentRequest;
use templiqx_docx_v5::DocxV5Adapter;

fn corpus_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/legacy-corpus/fixtures")
}

fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

#[test]
fn inspect_document_reports_v5_nested_table_without_writing() -> Result<()> {
    let fixture = corpus_root().join("v5-nested-table");
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("demo"))?;
    fs::copy(
        fixture.join("source.docx"),
        root.path().join("demo/source.docx"),
    )?;
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(root.path(), workspace.path())?;

    let envelope = service.inspect_document(&InspectDocumentRequest {
        package: "demo".into(),
        dialect: "v5".into(),
        template: "source.docx".into(),
        aliases: read_json(&fixture.join("aliases.json"))?,
    });
    ensure!(envelope.ok, "{:?}", envelope.diagnostics);
    let report = envelope.result.context("inspect result")?.report;
    let expected = read_json(&fixture.join("expected-report.json"))?;
    ensure!(report == expected, "report mismatch\n{report:#}");
    ensure!(
        !workspace.path().join("demo").exists(),
        "inspection must not create workspace artifacts"
    );
    Ok(())
}

#[test]
fn inspect_document_matches_adapter_analyze() -> Result<()> {
    let fixture = corpus_root().join("v5-header-footer");
    let template = fixture.join("source.docx");
    let aliases = read_json(&fixture.join("aliases.json"))?;
    let adapter = DocxV5Adapter::default();
    let direct = adapter.analyze(&template, &aliases)?;
    let direct = serde_json::to_value(direct)?;

    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("demo"))?;
    fs::copy(&template, root.path().join("demo/source.docx"))?;
    let service = templiqx_local::compose_with_workspace(root.path(), tempfile::tempdir()?.path())?;
    let envelope = service.inspect_document(&InspectDocumentRequest {
        package: "demo".into(),
        dialect: "v5".into(),
        template: "source.docx".into(),
        aliases,
    });
    ensure!(envelope.ok, "{:?}", envelope.diagnostics);
    ensure!(
        envelope.result.context("inspect result")?.report == direct,
        "service report must match DocxV5Adapter::analyze"
    );
    Ok(())
}

#[test]
fn inspect_document_rejects_unconfined_template_paths() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("demo"))?;
    fs::copy(
        corpus_root().join("v5-nested-table/source.docx"),
        root.path().join("demo/source.docx"),
    )?;
    let service = templiqx_local::compose_with_workspace(root.path(), tempfile::tempdir()?.path())?;

    for template in ["../demo/source.docx", r"demo\source.docx"] {
        let envelope = service.inspect_document(&InspectDocumentRequest {
            package: "demo".into(),
            dialect: "v5".into(),
            template: template.into(),
            aliases: serde_json::json!({}),
        });
        ensure!(!envelope.ok, "accepted unsafe template path {template}");
        ensure!(
            envelope
                .diagnostics
                .iter()
                .any(|d| d.code == "TQX_PATH_INVALID"),
            "{:?}",
            envelope.diagnostics
        );
    }
    Ok(())
}
