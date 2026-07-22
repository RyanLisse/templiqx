use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use serde_json::Value;
use std::path::PathBuf;
use templiqx_application::CreatePackageRequest;
use tower::ServiceExt;

async fn request(path: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");
    let response = app
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = serde_json::from_slice(&bytes).expect("json body");
    (status, headers, body)
}

#[tokio::test]
async fn health_returns_api_version_and_request_id() {
    let (status, headers, body) = request("/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["api_version"], "templiqx/v1alpha1");
    assert!(headers.contains_key("x-request-id"));
}

#[tokio::test]
async fn catalog_returns_operation_envelope() {
    let (status, _headers, body) = request("/operations/v1/catalog").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["operation"], "catalog");
    assert_eq!(body["ok"], true);
    let data = body["result"].as_array().expect("catalog data");
    assert!(data.iter().any(|operation| operation == "inspect_contract"));
    assert!(data.iter().any(|operation| operation == "execute_contract"));
}

#[tokio::test]
async fn discover_returns_empty_package_envelope_for_empty_root() {
    let (status, _headers, body) = request("/operations/v1/packages").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["operation"], "discover_packages");
    assert_eq!(body["ok"], true);
    assert_eq!(body["result"].as_array().expect("packages").len(), 0);
}

#[tokio::test]
async fn serves_the_checked_in_openapi_document_as_yaml_and_json() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");

    let yaml = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/operations/v1/openapi.yaml")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(yaml.status(), StatusCode::OK);
    assert_eq!(yaml.headers()["content-type"], "application/yaml");
    let yaml_body = to_bytes(yaml.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    assert!(yaml_body.starts_with(b"openapi: 3.1.0"));

    let json = app
        .oneshot(
            Request::builder()
                .uri("/operations/v1/openapi.json")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(json.status(), StatusCode::OK);
    let json_body = to_bytes(json.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let document: Value = serde_json::from_slice(&json_body).expect("openapi json");
    assert_eq!(document["openapi"], "3.1.0");
    assert_eq!(document["info"]["title"], "Templiqx Operations API");
}

#[tokio::test]
async fn package_update_requires_and_applies_cas_fingerprint() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let created = service.create_package(&CreatePackageRequest {
        name: "demo".into(),
        version: "0.1.0".into(),
    });
    let fingerprint = created.fingerprints["package"].clone();
    let response = templiqx_http::router(service)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/operations/v1/packages/demo")
                .header("content-type", "application/json")
                .header("if-match", fingerprint)
                .body(Body::from(r#"{"version":"0.2.0"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body: Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(body["operation"], "update_package");
    assert_eq!(body["result"]["version"], "0.2.0");
}

#[tokio::test]
async fn compile_accepts_the_openapi_request_shape() {
    let packages = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/packages");
    let response = templiqx_http::router_from_root(packages)
        .expect("compose router")
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/operations/v1/packages/demo/contracts/greeting/compile")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"render":{"inputs":{"name":"Ryan"},"context":{"organization":"Blinqx"}},"capabilities":["structured_output"]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body: Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(body["operation"], "compile_contract");
    assert_eq!(body["ok"], true);
}

#[tokio::test]
async fn every_catalog_operation_has_a_reachable_http_route() {
    let cases = [
        (Method::GET, "/operations/v1/catalog", "", "catalog"),
        (
            Method::GET,
            "/operations/v1/packages",
            "",
            "discover_packages",
        ),
        (
            Method::POST,
            "/operations/v1/packages",
            r#"{"name":"demo","version":"0.1.0"}"#,
            "create_package",
        ),
        (
            Method::PATCH,
            "/operations/v1/packages/demo",
            r#"{"version":"0.2.0"}"#,
            "update_package",
        ),
        (
            Method::DELETE,
            "/operations/v1/packages/demo",
            "",
            "delete_package",
        ),
        (
            Method::GET,
            "/operations/v1/packages/demo/identity",
            "",
            "export_package_identity",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/sign",
            r#"{"key_id":"dev"}"#,
            "sign_package",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/verify-trust",
            r#"{"strict":false}"#,
            "verify_package_trust",
        ),
        (
            Method::GET,
            "/operations/v1/packages/demo/contracts/greeting",
            "",
            "inspect_contract",
        ),
        (
            Method::PUT,
            "/operations/v1/packages/demo/contracts/greeting",
            "api_version: templiqx/v1alpha1",
            "put_contract",
        ),
        (
            Method::DELETE,
            "/operations/v1/packages/demo/contracts/greeting",
            "",
            "delete_contract",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/contracts/greeting/validate",
            "",
            "validate_contract",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/validate",
            "",
            "validate_package",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/contracts/greeting/compile",
            "{}",
            "compile_contract",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/contracts/greeting/render",
            "{}",
            "render_contract",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/contracts/greeting/execute",
            "{}",
            "execute_contract",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/test",
            "{}",
            "test_package",
        ),
        (
            Method::GET,
            "/operations/v1/packages/demo/evals",
            "",
            "list_evals",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/evals/run",
            r#"{"contract":"greeting","fixture_id":"basic"}"#,
            "run_eval",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/quality/proposals:assess",
            "{}",
            "assess_quality_proposals",
        ),
        (
            Method::POST,
            "/operations/v1/packages/demo/contracts/greeting/diff",
            r#"{"right_package":"demo","right_contract":"other"}"#,
            "diff_contract",
        ),
        (
            Method::GET,
            "/operations/v1/packages/demo/contracts/greeting/explain",
            "",
            "explain_contract",
        ),
        (
            Method::POST,
            "/operations/v1/legacy/migrate",
            r#"{"package":"demo","dialect":"jinja","source":"legacy.txt","aliases":{}}"#,
            "migrate_legacy",
        ),
        (
            Method::POST,
            "/operations/v1/documents/render",
            r#"{"package":"demo","template":"template.docx","data":{},"output":"result.docx"}"#,
            "render_document",
        ),
        (
            Method::POST,
            "/operations/v1/documents/inspect",
            r#"{"package":"demo","dialect":"v5","template":"template.docx","aliases":{}}"#,
            "inspect_document",
        ),
        (
            Method::GET,
            "/operations/v1/artifacts?package=demo",
            "",
            "list_workspace_artifacts",
        ),
        (
            Method::GET,
            "/operations/v1/artifacts/reports/result.txt?package=demo",
            "",
            "read_artifact",
        ),
        (
            Method::DELETE,
            "/operations/v1/artifacts/reports/result.txt?package=demo",
            "",
            "delete_workspace_artifact",
        ),
    ];

    assert_eq!(cases.len(), 28);
    for (method, uri, request_body, operation) in cases {
        let root = tempfile::tempdir().expect("temp root");
        let response = templiqx_http::router_from_root(root.path())
            .expect("compose router")
            .oneshot(
                Request::builder()
                    .method(method.clone())
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_ne!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri} is not routed"
        );
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let body: Value = serde_json::from_slice(&bytes).expect("operation envelope");
        assert_eq!(
            body["operation"], operation,
            "wrong handler for {method} {uri}"
        );
    }
}
