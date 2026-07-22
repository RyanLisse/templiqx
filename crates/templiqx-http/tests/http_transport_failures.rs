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

fn quality_request(package: &str) -> Value {
    json!({
        "package": package,
        "contract_id": "contract",
        "expected_package_fingerprint": "package-fingerprint",
        "expected_base_contract_fingerprint": "contract-fingerprint",
        "expected_fixture_set_fingerprint": "fixture-fingerprint",
        "policy": {
            "id": "policy",
            "replicates_per_fixture": 1,
            "minimum_semantic_cases": 1,
            "maximum_infrastructure_failure_ppm": 0,
            "claimed_evaluator_profile_fingerprint": "evaluator-fingerprint",
            "claimed_model_profile_fingerprint": "model-fingerprint",
            "binary_scorers": [],
            "objectives": [],
            "eligibility_rules": []
        },
        "candidates": []
    })
}

fn assert_fixed_quality_json_rejection(body: &Value) {
    assert_eq!(body["operation"], "assess_quality_proposals");
    assert_eq!(body["ok"], false);
    assert_eq!(body["diagnostics"][0]["code"], "TQX_HTTP_QUALITY_JSON");
    assert_eq!(
        body["diagnostics"][0]["message"],
        "quality assessment request body is invalid"
    );
    assert_eq!(body["diagnostics"][0]["json_pointer"], "/");
}

fn assert_fixed_payload_too_large(body: &Value) {
    assert_eq!(
        body,
        &json!({
            "code": "TQX_TRANSPORT_PAYLOAD_TOO_LARGE",
            "message": "request body exceeds the maximum allowed size"
        })
    );
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
    assert!(
        body["diagnostics"][0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unexpected")),
        "legacy routes must retain detailed JSON rejection messages: {body}"
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
async fn quality_assessment_rejects_unknown_fields_fail_closed() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");
    let (status, _headers, body) = request_json(
        app,
        Method::POST,
        "/operations/v1/packages/demo/quality/proposals:assess",
        Some(r#"{"customer_ryan.sensitive@example.invalid_ssn_123-45-6789":true}"#),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_fixed_quality_json_rejection(&body);
    let encoded = serde_json::to_string(&body).expect("response JSON");
    assert!(
        !encoded.contains("ryan.sensitive@example.invalid")
            && !encoded.contains("123-45-6789")
            && !encoded.contains("unknown field"),
        "quality rejection must not echo candidate-controlled field names: {body}"
    );
}

#[tokio::test]
async fn quality_assessment_redacts_malformed_json_parser_details() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");
    let malformed = r#"{"package":"demo","candidate_ryan.sensitive@example.invalid_ssn_123-45-6789":"unterminated"#;
    let (status, _headers, body) = request_json(
        app,
        Method::POST,
        "/operations/v1/packages/demo/quality/proposals:assess",
        Some(malformed),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_fixed_quality_json_rejection(&body);
    let encoded = serde_json::to_string(&body).expect("response JSON");
    assert!(
        !encoded.contains("ryan.sensitive@example.invalid")
            && !encoded.contains("123-45-6789")
            && !encoded.contains("unterminated")
            && !encoded.contains("EOF"),
        "quality rejection must not echo submitted values or parser details: {body}"
    );
}

#[tokio::test]
async fn quality_assessment_requires_body_and_route_package_to_match() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");
    let request = quality_request("other");
    let encoded = serde_json::to_string(&request).expect("request JSON");
    let (status, _headers, body) = request_json(
        app,
        Method::POST,
        "/operations/v1/packages/demo/quality/proposals:assess",
        Some(&encoded),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["operation"], "assess_quality_proposals");
    assert_eq!(
        body["diagnostics"][0]["code"],
        "TQX_QUALITY_BINDING_MISMATCH"
    );
    assert_eq!(body["diagnostics"][0]["json_pointer"], "/package");
}

#[tokio::test]
async fn quality_assessment_has_a_route_scoped_four_mib_body_limit() {
    let root = tempfile::tempdir().expect("temp root");
    let app = templiqx_http::router_from_root(root.path()).expect("compose router");

    // More than the default 1 MiB must reach application validation on this route.
    let mut within_quality_limit = quality_request("demo");
    within_quality_limit["policy"]["id"] = Value::String("x".repeat(1024 * 1024 + 1));
    let within_quality_limit =
        serde_json::to_string(&within_quality_limit).expect("within-limit request JSON");
    let (status, _headers, body) = request_json(
        app.clone(),
        Method::POST,
        "/operations/v1/packages/demo/quality/proposals:assess",
        Some(&within_quality_limit),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .all(|diagnostic| diagnostic["code"] != "TQX_HTTP_QUALITY_JSON"),
        "the quality route must not retain the default 1 MiB limit: {body}"
    );

    let email = "ryan.sensitive@example.invalid";
    let ssn = "123-45-6789";
    let mut over_quality_limit = quality_request("demo");
    over_quality_limit["policy"]["id"] =
        Value::String(format!("{email}_{ssn}_{}", "x".repeat(4 * 1024 * 1024 + 1)));
    let over_quality_limit =
        serde_json::to_string(&over_quality_limit).expect("over-limit request JSON");
    let (status, _headers, body) = request_json(
        app,
        Method::POST,
        "/operations/v1/packages/demo/quality/proposals:assess",
        Some(&over_quality_limit),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_fixed_payload_too_large(&body);
    let encoded = serde_json::to_string(&body).expect("response JSON");
    assert!(
        !encoded.contains(email) && !encoded.contains(ssn),
        "payload-too-large rejection reflected request data: {body}"
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
