use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn swagger_ui_serves_html_and_points_at_checked_in_openapi_json() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");

    let ui = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/swagger-ui/")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert!(
        ui.status() == StatusCode::OK || ui.status().is_redirection(),
        "expected Swagger UI success or redirect, got {}",
        ui.status()
    );
    let ui_status = ui.status();
    let ui_bytes = to_bytes(ui.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let ui_body = String::from_utf8_lossy(&ui_bytes);
    assert!(
        ui_body.to_ascii_lowercase().contains("swagger") || ui_status.is_redirection(),
        "Swagger UI body should mention swagger (or redirect to index)"
    );

    let openapi = app
        .oneshot(
            Request::builder()
                .uri("/operations/v1/openapi.json")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(openapi.status(), StatusCode::OK);
    let openapi_bytes = to_bytes(openapi.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let document: serde_json::Value = serde_json::from_slice(&openapi_bytes).expect("openapi json");
    assert_eq!(document["openapi"], "3.1.0");
    assert!(
        document["paths"]
            .as_object()
            .expect("paths")
            .contains_key("/operations/v1/catalog"),
        "checked-in OpenAPI must still expose catalog"
    );
}
