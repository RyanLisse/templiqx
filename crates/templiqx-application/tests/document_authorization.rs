//! Application boundary tests for document-render authorization.

use serde_json::{Value, json};
use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use templiqx_application::{RenderDocumentRequest, TempliqxService, binding_fingerprint};
use templiqx_contracts::{AUTHORIZED_MERGE_CONTEXT_KEY, AuthorizedMergeContext, PackageManifest};
use templiqx_local::{
    DeterministicFakeRuntime, FilesystemArtifactWorkspace, FilesystemPackageStore,
    UnsupportedDocumentInspector, UnsupportedLegacyAdapter,
};
use templiqx_ports::{DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, PortError};

#[derive(Clone, Default)]
struct CapturingRenderer {
    calls: Arc<AtomicUsize>,
    data: Arc<Mutex<Option<Value>>>,
}

impl DocumentRenderer for CapturingRenderer {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.data.lock().expect("capture lock") = Some(request.data.clone());
        fs::write(&request.output, b"rendered")
            .map_err(|error| PortError::Io(error.to_string()))?;
        Ok(DocumentRenderResult {
            artifact: request.output.clone(),
            report: json!({}),
        })
    }
}

fn service(
    root: &std::path::Path,
    workspace: &std::path::Path,
    renderer: CapturingRenderer,
) -> TempliqxService<
    FilesystemPackageStore,
    FilesystemArtifactWorkspace,
    DeterministicFakeRuntime,
    UnsupportedLegacyAdapter,
    CapturingRenderer,
    UnsupportedDocumentInspector,
> {
    TempliqxService::new(
        FilesystemPackageStore::new(root).expect("package store"),
        FilesystemArtifactWorkspace::new(workspace).expect("artifact workspace"),
        DeterministicFakeRuntime,
        UnsupportedLegacyAdapter,
        renderer,
        UnsupportedDocumentInspector,
    )
}

fn package(root: &std::path::Path, requirement: Option<&str>) {
    templiqx_local::create_package(root, "demo", "0.1.0").expect("create package");
    fs::write(root.join("demo/template.docx"), b"template").expect("write template");
    if let Some(requirement) = requirement {
        let manifest_path = root.join("demo/templiqx.yaml");
        let mut manifest: PackageManifest =
            serde_yaml_ng::from_str(&fs::read_to_string(&manifest_path).expect("read manifest"))
                .expect("parse manifest");
        manifest
            .provenance
            .insert("requires_authorized_context".into(), requirement.into());
        fs::write(
            manifest_path,
            serde_yaml_ng::to_string(&manifest).expect("serialize manifest"),
        )
        .expect("write manifest");
    }
}

fn authorized_context() -> AuthorizedMergeContext {
    let mut context = AuthorizedMergeContext {
        scope_id: "SYN-SCOPE-001".into(),
        policy_decision_id: "SYN-POLICY-DEC-001".into(),
        policy_version: "1.0.0".into(),
        evidence_provenance_id: "SYN-EVID-PROV-001".into(),
        issued_at: "2026-07-15T10:00:00Z".into(),
        expires_at: "2099-12-31T23:59:59Z".into(),
        fingerprint: String::new(),
    };
    context.fingerprint = binding_fingerprint(&context).expect("context fingerprint");
    context
}

#[test]
fn render_document_propagates_manifest_load_failure_before_rendering() {
    let root = tempfile::tempdir().expect("root");
    let workspace = tempfile::tempdir().expect("workspace");
    package(root.path(), None);
    fs::write(root.path().join("demo/templiqx.yaml"), "not: [valid").expect("malform manifest");
    let renderer = CapturingRenderer::default();
    let calls = Arc::clone(&renderer.calls);
    let envelope =
        service(root.path(), workspace.path(), renderer).render_document(&RenderDocumentRequest {
            package: "demo".into(),
            template: "template.docx".into(),
            data: json!({"client_name": "Acme"}),
            output: "output.docx".into(),
            workspace: None,
        });

    assert!(!envelope.ok);
    assert!(
        envelope
            .diagnostics
            .iter()
            .any(|d| d.code == "TQX_DATA_INVALID")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn render_document_strips_authorization_metadata_before_adapter() {
    let root = tempfile::tempdir().expect("root");
    let workspace = tempfile::tempdir().expect("workspace");
    package(root.path(), Some("true"));
    let renderer = CapturingRenderer::default();
    let captured_data = Arc::clone(&renderer.data);
    let envelope =
        service(root.path(), workspace.path(), renderer).render_document(&RenderDocumentRequest {
            package: "demo".into(),
            template: "template.docx".into(),
            data: json!({
                "client_name": "Acme",
                AUTHORIZED_MERGE_CONTEXT_KEY: authorized_context(),
            }),
            output: "output.docx".into(),
            workspace: None,
        });

    assert!(envelope.ok, "{:?}", envelope.diagnostics);
    assert_eq!(
        captured_data.lock().expect("capture lock").as_ref(),
        Some(&json!({"client_name": "Acme"}))
    );
}

#[test]
fn render_document_rejects_invalid_authorization_requirement() {
    let root = tempfile::tempdir().expect("root");
    let workspace = tempfile::tempdir().expect("workspace");
    package(root.path(), Some("yes"));
    let renderer = CapturingRenderer::default();
    let calls = Arc::clone(&renderer.calls);
    let envelope =
        service(root.path(), workspace.path(), renderer).render_document(&RenderDocumentRequest {
            package: "demo".into(),
            template: "template.docx".into(),
            data: json!({"client_name": "Acme"}),
            output: "output.docx".into(),
            workspace: None,
        });

    assert!(!envelope.ok);
    assert!(
        envelope
            .diagnostics
            .iter()
            .any(|d| d.code == "TQX_AUTHORIZED_CONTEXT_REQUIREMENT_INVALID")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
