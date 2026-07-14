use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use templiqx_application::{
    CreatePackageRequest, DeleteContractRequest, DeletePackageRequest,
    DeleteWorkspaceArtifactRequest, ListWorkspaceArtifactsRequest, MigrateLegacyRequest,
    ReadArtifactRequest, RenderDocumentRequest, SignPackageRequest, UpdatePackageRequest,
    VerifyPackageTrustRequest,
};
use templiqx_contracts::{OperationEnvelope, RenderRequest};
use tower::ServiceExt;

const INTERACTION_BODY: &str = r#"{"render":{"inputs":{"name":"Ryan"},"context":{"organization":"Blinqx"}},"capabilities":["structured_output"]}"#;
const EXECUTE_BODY: &str = r#"{"render":{"inputs":{"name":"Ryan"},"context":{"organization":"Blinqx"}},"capabilities":["structured_output"],"fixture_output":{"greeting":"Hello Ryan"},"stream":false}"#;

fn packages_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/packages")
}

fn greeting_source() -> String {
    std::fs::read_to_string(packages_root().join("demo/contracts/greeting.yaml"))
        .expect("greeting fixture")
}

fn greeting_render() -> RenderRequest {
    RenderRequest {
        inputs: [("name".into(), json!("Ryan"))].into(),
        context: [("organization".into(), json!("Blinqx"))].into(),
    }
}

fn create_empty_package(root: &Path) -> (templiqx_local::LocalService, String) {
    let service = templiqx_local::compose(root).expect("compose service");
    let created = service.create_package(&CreatePackageRequest {
        name: "demo".into(),
        version: "0.1.0".into(),
    });
    assert!(created.ok, "package setup must succeed: {created:?}");
    let fingerprint = created
        .fingerprints
        .get("package")
        .expect("package fingerprint")
        .clone();
    (service, fingerprint)
}

fn create_package_with_contract(root: &Path) -> (templiqx_local::LocalService, String) {
    let (service, _) = create_empty_package(root);
    let put = service.put_contract("demo", "greeting", &greeting_source(), None);
    assert!(put.ok, "contract setup must succeed: {put:?}");
    let fingerprint = put
        .fingerprints
        .get("contract")
        .expect("contract fingerprint")
        .clone();
    (service, fingerprint)
}

async fn http_envelope_bytes(
    service: templiqx_local::LocalService,
    method: Method,
    uri: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Vec<u8>) {
    router_envelope_bytes(templiqx_http::router(service), method, uri, body, headers).await
}

async fn router_envelope_bytes(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if !body.is_empty() {
        builder = builder.header("content-type", "application/json");
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .oneshot(
            builder
                .body(if body.is_empty() {
                    Body::empty()
                } else {
                    Body::from(body.to_owned())
                })
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes")
        .to_vec();
    (status, bytes)
}

async fn http_envelope(
    service: templiqx_local::LocalService,
    method: Method,
    uri: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> Value {
    let (status, bytes) = http_envelope_bytes(service, method.clone(), uri, body, headers).await;
    let envelope: Value = serde_json::from_slice(&bytes).expect("operation envelope json");
    assert_eq!(
        status.is_success(),
        envelope["ok"].as_bool().expect("envelope ok flag"),
        "HTTP status and envelope outcome disagree for {method} {uri}: {envelope}"
    );
    envelope
}

fn assert_http_matches_service<T: Serialize>(service_value: &T, http_value: &Value) {
    let service_value = serde_json::to_value(service_value).expect("service envelope");
    assert_eq!(
        service_value, *http_value,
        "HTTP transport must return the same envelope as TempliqxService"
    );
}

fn assert_happy_http_matches_service<T: Serialize>(
    service_value: &OperationEnvelope<T>,
    http_value: &Value,
) {
    assert!(
        service_value.ok,
        "happy-path fixture must succeed: {:?}",
        service_value.diagnostics
    );
    assert_http_matches_service(service_value, http_value);
}

#[tokio::test]
async fn catalog_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let direct = service.catalog();
    let http = http_envelope(service, Method::GET, "/operations/v1/catalog", "", &[]).await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn discover_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.discover_packages();
    let http = http_envelope(service, Method::GET, "/operations/v1/packages", "", &[]).await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn create_package_envelope_matches_direct_service_call() {
    let direct_root = tempfile::tempdir().expect("temp root");
    let http_root = tempfile::tempdir().expect("temp root");
    let direct_service = templiqx_local::compose(direct_root.path()).expect("compose service");
    let http_service = templiqx_local::compose(http_root.path()).expect("compose service");
    let request = CreatePackageRequest {
        name: "demo".into(),
        version: "0.1.0".into(),
    };
    let direct = direct_service.create_package(&request);
    let http = http_envelope(
        http_service,
        Method::POST,
        "/operations/v1/packages",
        r#"{"name":"demo","version":"0.1.0"}"#,
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn update_package_envelope_matches_direct_service_call() {
    let direct_root = tempfile::tempdir().expect("temp root");
    let http_root = tempfile::tempdir().expect("temp root");
    let (direct_service, direct_fingerprint) = create_empty_package(direct_root.path());
    let (http_service, http_fingerprint) = create_empty_package(http_root.path());
    assert_eq!(direct_fingerprint, http_fingerprint);
    let direct = direct_service.update_package(&UpdatePackageRequest {
        package: "demo".into(),
        version: Some("0.2.0".into()),
        description: Some("updated".into()),
        expected_fingerprint: direct_fingerprint,
    });
    let http = http_envelope(
        http_service,
        Method::PATCH,
        "/operations/v1/packages/demo",
        r#"{"version":"0.2.0","description":"updated"}"#,
        &[("if-match", http_fingerprint.as_str())],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn delete_package_envelope_matches_direct_service_call() {
    let direct_root = tempfile::tempdir().expect("temp root");
    let http_root = tempfile::tempdir().expect("temp root");
    let (direct_service, direct_fingerprint) = create_empty_package(direct_root.path());
    let (http_service, http_fingerprint) = create_empty_package(http_root.path());
    assert_eq!(direct_fingerprint, http_fingerprint);
    let direct = direct_service.delete_package(&DeletePackageRequest {
        package: "demo".into(),
        expected_fingerprint: direct_fingerprint,
    });
    let http = http_envelope(
        http_service,
        Method::DELETE,
        "/operations/v1/packages/demo",
        "",
        &[("if-match", http_fingerprint.as_str())],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn export_package_identity_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.export_package_identity("demo");
    let http = http_envelope(
        service,
        Method::GET,
        "/operations/v1/packages/demo/identity",
        "",
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn sign_package_error_envelope_matches_direct_service_call() {
    let direct_root = tempfile::tempdir().expect("temp root");
    let http_root = tempfile::tempdir().expect("temp root");
    let direct_service = templiqx_local::compose(direct_root.path()).expect("compose service");
    let http_service = templiqx_local::compose(http_root.path()).expect("compose service");
    let direct = direct_service.sign_package(&SignPackageRequest {
        package: "missing".into(),
        key_id: "dev".into(),
        expected_fingerprint: "sha256:deadbeef".into(),
    });
    let http = http_envelope(
        http_service,
        Method::POST,
        "/operations/v1/packages/missing/sign",
        r#"{"key_id":"dev"}"#,
        &[("if-match", "sha256:deadbeef")],
    )
    .await;
    assert!(!direct.ok, "signing error fixture must fail");
    assert_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn verify_package_trust_error_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let direct = service.verify_package_trust(&VerifyPackageTrustRequest {
        package: "missing".into(),
        strict: true,
    });
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/missing/verify-trust",
        r#"{"strict":true}"#,
        &[],
    )
    .await;
    assert!(!direct.ok, "trust error fixture must fail");
    assert_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn inspect_contract_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.inspect_contract("demo", "greeting");
    let http = http_envelope(
        service,
        Method::GET,
        "/operations/v1/packages/demo/contracts/greeting",
        "",
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn put_contract_envelope_matches_direct_service_call() {
    let direct_root = tempfile::tempdir().expect("temp root");
    let http_root = tempfile::tempdir().expect("temp root");
    let (direct_service, _) = create_empty_package(direct_root.path());
    let (http_service, _) = create_empty_package(http_root.path());
    let source = greeting_source();
    let direct = direct_service.put_contract("demo", "greeting", &source, None);
    let http = http_envelope(
        http_service,
        Method::PUT,
        "/operations/v1/packages/demo/contracts/greeting",
        &source,
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn delete_contract_envelope_matches_direct_service_call() {
    let direct_root = tempfile::tempdir().expect("temp root");
    let http_root = tempfile::tempdir().expect("temp root");
    let (direct_service, direct_fingerprint) = create_package_with_contract(direct_root.path());
    let (http_service, http_fingerprint) = create_package_with_contract(http_root.path());
    assert_eq!(direct_fingerprint, http_fingerprint);
    let direct = direct_service.delete_contract(&DeleteContractRequest {
        package: "demo".into(),
        contract: "greeting".into(),
        expected_fingerprint: direct_fingerprint,
    });
    let http = http_envelope(
        http_service,
        Method::DELETE,
        "/operations/v1/packages/demo/contracts/greeting",
        "",
        &[("if-match", http_fingerprint.as_str())],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn validate_contract_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.validate_contract("demo", "greeting");
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/contracts/greeting/validate",
        "",
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn validate_package_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.validate_package("demo");
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/validate",
        "",
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn compile_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let render = greeting_render();
    let capabilities = vec!["structured_output".into()];
    let direct = service.compile_contract("demo", "greeting", &render, &capabilities);
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/contracts/greeting/compile",
        INTERACTION_BODY,
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn render_contract_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let render = greeting_render();
    let capabilities = vec!["structured_output".into()];
    let direct = service.render_contract("demo", "greeting", &render, &capabilities);
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/contracts/greeting/render",
        INTERACTION_BODY,
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn execute_contract_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let render = greeting_render();
    let capabilities = vec!["structured_output".into()];
    let direct = service.execute_contract(
        "demo",
        "greeting",
        &render,
        &capabilities,
        Some(json!({"greeting": "Hello Ryan"})),
        false,
    );
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/contracts/greeting/execute",
        EXECUTE_BODY,
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn migrate_legacy_error_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let request = MigrateLegacyRequest {
        package: "missing".into(),
        dialect: "v5".into(),
        source: "missing.docx".into(),
        aliases: json!({}),
    };
    let direct = service.migrate_legacy(&request);
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/legacy/migrate",
        r#"{"package":"missing","dialect":"v5","source":"missing.docx","aliases":{}}"#,
        &[],
    )
    .await;
    assert!(!direct.ok, "migration error fixture must fail");
    assert_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn render_document_error_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let request = RenderDocumentRequest {
        package: "missing".into(),
        template: "missing.docx".into(),
        data: json!({}),
        output: "result.docx".into(),
        workspace: None,
    };
    let direct = service.render_document(&request);
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/documents/render",
        r#"{"package":"missing","template":"missing.docx","data":{},"output":"result.docx"}"#,
        &[],
    )
    .await;
    assert!(!direct.ok, "document render error fixture must fail");
    assert_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn list_workspace_artifacts_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let direct = service.list_workspace_artifacts(&ListWorkspaceArtifactsRequest {
        package: "demo".into(),
        workspace: None,
        prefix: None,
    });
    let http = http_envelope(
        service,
        Method::GET,
        "/operations/v1/artifacts?package=demo",
        "",
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn read_artifact_error_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let direct = service.read_artifact(&ReadArtifactRequest {
        package: "demo".into(),
        path: "missing.txt".into(),
        workspace: None,
    });
    let http = http_envelope(
        service,
        Method::GET,
        "/operations/v1/artifacts/missing.txt?package=demo",
        "",
        &[],
    )
    .await;
    assert!(!direct.ok, "artifact read error fixture must fail");
    assert_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn delete_workspace_artifact_error_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let direct = service.delete_workspace_artifact(&DeleteWorkspaceArtifactRequest {
        package: "demo".into(),
        path: "missing.txt".into(),
        workspace: None,
        expected_fingerprint: "sha256:deadbeef".into(),
    });
    let http = http_envelope(
        service,
        Method::DELETE,
        "/operations/v1/artifacts/missing.txt?package=demo",
        "",
        &[("if-match", "sha256:deadbeef")],
    )
    .await;
    assert!(!direct.ok, "artifact delete error fixture must fail");
    assert_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn test_package_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let capabilities = vec!["structured_output".into()];
    let direct = service.test_package("demo", &capabilities);
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/test",
        r#"{"capabilities":["structured_output"]}"#,
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn list_evals_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.list_evals("demo");
    let http = http_envelope(
        service,
        Method::GET,
        "/operations/v1/packages/demo/evals",
        "",
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn run_eval_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let capabilities = vec!["structured_output".into()];
    let direct = service.run_eval("demo", "greeting", "ryan", &capabilities);
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/evals/run",
        r#"{"contract":"greeting","fixture_id":"ryan","capabilities":["structured_output"]}"#,
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn diff_contract_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.diff_contract("demo", "greeting", "demo", "greeting");
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/contracts/greeting/diff",
        r#"{"right_package":"demo","right_contract":"greeting"}"#,
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn explain_contract_envelope_matches_direct_service_call() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.explain_contract("demo", "greeting");
    let http = http_envelope(
        service,
        Method::GET,
        "/operations/v1/packages/demo/contracts/greeting/explain",
        "",
        &[],
    )
    .await;
    assert_happy_http_matches_service(&direct, &http);
}

async fn assert_repeated_http_is_byte_identical(
    uri: &str,
    body: &str,
    direct_fingerprints: &impl Serialize,
) {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let app = templiqx_http::router(service);
    let mut repeats = Vec::new();
    for _ in 0..3 {
        let (status, bytes) =
            router_envelope_bytes(app.clone(), Method::POST, uri, body, &[]).await;
        assert_eq!(status, StatusCode::OK);
        repeats.push(bytes);
    }
    assert_eq!(
        repeats[0], repeats[1],
        "first two envelopes differ by bytes"
    );
    assert_eq!(
        repeats[0], repeats[2],
        "first and third envelopes differ by bytes"
    );

    let direct_fingerprints = serde_json::to_value(direct_fingerprints).expect("fingerprints");
    for bytes in repeats {
        let envelope: Value = serde_json::from_slice(&bytes).expect("operation envelope");
        assert_eq!(
            envelope["fingerprints"], direct_fingerprints,
            "HTTP fingerprints must match the direct service call"
        );
    }
}

#[tokio::test]
async fn compile_contract_http_envelope_is_byte_deterministic() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.compile_contract(
        "demo",
        "greeting",
        &greeting_render(),
        &["structured_output".into()],
    );
    assert!(direct.ok, "compile determinism fixture must succeed");
    assert_repeated_http_is_byte_identical(
        "/operations/v1/packages/demo/contracts/greeting/compile",
        INTERACTION_BODY,
        &direct.fingerprints,
    )
    .await;
}

#[tokio::test]
async fn execute_contract_http_envelope_is_byte_deterministic() {
    let service = templiqx_local::compose(packages_root()).expect("compose service");
    let direct = service.execute_contract(
        "demo",
        "greeting",
        &greeting_render(),
        &["structured_output".into()],
        Some(json!({"greeting": "Hello Ryan"})),
        false,
    );
    assert!(direct.ok, "execute determinism fixture must succeed");
    assert_repeated_http_is_byte_identical(
        "/operations/v1/packages/demo/contracts/greeting/execute",
        EXECUTE_BODY,
        &direct.fingerprints,
    )
    .await;
}
