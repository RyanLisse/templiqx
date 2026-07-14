use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use serde::Serialize;
use serde_json::{Value, json};
use templiqx_application::CreatePackageRequest;
use tower::ServiceExt;

async fn http_envelope(
    service: templiqx_local::LocalService,
    method: Method,
    uri: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> Value {
    let mut builder = Request::builder().method(method.clone()).uri(uri);
    if !body.is_empty() {
        builder = builder.header("content-type", "application/json");
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = templiqx_http::router(service)
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
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "HTTP parity request failed for {method} {uri}"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    serde_json::from_slice(&bytes).expect("operation envelope json")
}

fn assert_http_matches_service<T: Serialize>(service_value: &T, http_value: &Value) {
    let service_value = serde_json::to_value(service_value).expect("service envelope");
    assert_eq!(
        service_value, *http_value,
        "HTTP transport must return the same envelope as TempliqxService"
    );
}

#[tokio::test]
async fn catalog_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let direct = service.catalog();
    let http = http_envelope(service, Method::GET, "/operations/v1/catalog", "", &[]).await;
    assert_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn discover_envelope_matches_direct_service_call() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let direct = service.discover_packages();
    let http = http_envelope(service, Method::GET, "/operations/v1/packages", "", &[]).await;
    assert_http_matches_service(&direct, &http);
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
    assert_http_matches_service(&direct, &http);
}

#[tokio::test]
async fn compile_envelope_matches_direct_service_call() {
    let packages =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/packages");
    let service = templiqx_local::compose(&packages).expect("compose service");
    let render = templiqx_contracts::RenderRequest {
        inputs: [("name".into(), json!("Ryan"))].into(),
        context: [("organization".into(), json!("Blinqx"))].into(),
    };
    let capabilities = vec!["structured_output".into()];
    let direct = service.compile_contract("demo", "greeting", &render, &capabilities);
    let http = http_envelope(
        service,
        Method::POST,
        "/operations/v1/packages/demo/contracts/greeting/compile",
        r#"{"render":{"inputs":{"name":"Ryan"},"context":{"organization":"Blinqx"}},"capabilities":["structured_output"]}"#,
        &[],
    )
    .await;
    assert_http_matches_service(&direct, &http);
}
