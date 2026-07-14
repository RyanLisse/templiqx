use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use serde_json::{Value, json};
use templiqx_application::CreatePackageRequest;
use tower::ServiceExt;

async fn request_json(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: Option<&str>,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let request = if let Some(body) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .expect("request")
    } else {
        builder.body(Body::empty()).expect("request")
    };
    let response = app.oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes).to_string()}))
    };
    (status, headers, body)
}

#[tokio::test]
async fn rejects_unknown_json_fields_on_strict_request_bodies() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");
    let (status, _headers, body) = request_json(
        app,
        Method::POST,
        "/operations/v1/packages/demo/contracts/greeting/compile",
        Some(r#"{"unexpected":true}"#),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["operation"], "compile_contract");
    assert_eq!(body["ok"], false);
    assert!(
        body["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "TQX_HTTP_JSON")
    );
}

#[tokio::test]
async fn rejects_invalid_json_bodies() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");
    let (status, _headers, body) = request_json(
        app,
        Method::POST,
        "/operations/v1/packages",
        Some("{not-json"),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["operation"], "create_package");
    assert_eq!(body["ok"], false);
    assert!(
        body["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "TQX_HTTP_JSON")
    );
}

#[tokio::test]
async fn rejects_oversized_request_bodies() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");
    let oversized = "x".repeat(1024 * 1024 + 1);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/operations/v1/packages")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"name":"{oversized}","version":"0.1.0"}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");
    assert!(
        response.status() == StatusCode::PAYLOAD_TOO_LARGE
            || response.status() == StatusCode::BAD_REQUEST,
        "oversized payloads must be rejected, got {}",
        response.status()
    );
}

#[tokio::test]
async fn echoes_supplied_request_ids_and_generates_when_missing() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");

    let generated = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/operations/v1/catalog")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let generated_id = generated
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("generated request id");
    assert!(generated_id.starts_with("tqx-"));

    let echoed = app
        .oneshot(
            Request::builder()
                .uri("/operations/v1/catalog")
                .header("x-request-id", "client-trace-42")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        echoed
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("client-trace-42")
    );
}

#[tokio::test]
async fn cas_conflicts_return_conflict_status_and_diagnostics() {
    let root = tempfile::tempdir().expect("temp root");
    let service = templiqx_local::compose(root.path()).expect("compose service");
    let created = service.create_package(&CreatePackageRequest {
        name: "demo".into(),
        version: "0.1.0".into(),
    });
    assert!(created.ok, "package bootstrap must succeed");
    let app = templiqx_http::router(service);

    let (status, _headers, body) = request_json(
        app.clone(),
        Method::PATCH,
        "/operations/v1/packages/demo",
        Some(r#"{"version":"0.2.0"}"#),
        &[("if-match", "sha256:deadbeef")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["operation"], "update_package");
    assert_eq!(body["ok"], false);
    assert!(
        body["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| {
                diagnostic["code"]
                    .as_str()
                    .is_some_and(|code| code.contains("CAS") || code.contains("CONFLICT"))
            })
    );

    let missing_if_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/operations/v1/packages/demo")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"version":"0.2.0"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(missing_if_match.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_encoded_artifact_path_traversal_for_reads_and_deletes() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");
    let uri = "/operations/v1/artifacts/..%2f..%2f..%2fetc%2fpasswd?package=demo";

    for (method, headers, operation) in [
        (Method::GET, &[][..], "read_artifact"),
        (
            Method::DELETE,
            &[("if-match", "sha256:deadbeef")][..],
            "delete_workspace_artifact",
        ),
    ] {
        let (status, _headers, body) =
            request_json(app.clone(), method.clone(), uri, None, headers).await;
        eprintln!("{method} {uri} -> {status} {body}");
        assert!(
            !status.is_success(),
            "{method} traversal request must be rejected, got {status}: {body}"
        );
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["operation"], operation);
        assert_eq!(body["ok"], false);
        assert!(
            body["diagnostics"]
                .as_array()
                .expect("diagnostics")
                .iter()
                .any(|diagnostic| diagnostic["code"] == "TQX_PATH_INVALID"),
            "{method} traversal response must expose the path-safety diagnostic: {body}"
        );
    }
}
