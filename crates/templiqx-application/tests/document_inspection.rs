use std::fs;
use templiqx_application::InspectDocumentRequest;

#[test]
fn inspect_document_without_adapter_returns_stable_unsupported_envelope() {
    let root = tempfile::tempdir().expect("temp root");
    templiqx_local::create_package(root.path(), "demo", "0.1.0").expect("create package");
    fs::write(root.path().join("demo/template.docx"), b"not-a-docx").expect("write template");
    let service = templiqx_local::compose_core(root.path()).expect("compose");

    let envelope = service.inspect_document(&InspectDocumentRequest {
        package: "demo".into(),
        dialect: "v5".into(),
        template: "template.docx".into(),
        aliases: serde_json::json!({}),
    });

    assert!(!envelope.ok, "expected failure without inspector adapter");
    assert!(envelope.result.is_none());
    assert!(
        envelope
            .diagnostics
            .iter()
            .any(|d| d.code == "TQX_UNSUPPORTED"),
        "{:?}",
        envelope.diagnostics
    );
}
