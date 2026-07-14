use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
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
async fn serves_the_checked_in_openapi_document() {
    let root = tempfile::tempdir().expect("temp root");
    let response = templiqx_http::router_from_root(root.path())
        .expect("compose router")
        .oneshot(
            Request::builder()
                .uri("/operations/v1/openapi.yaml")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/yaml");
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    assert!(body.starts_with(b"openapi: 3.1.0"));
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
