use std::time::Duration;

use templiqx_adapter_rust::{CallOptions, Client, ClientOptions, TempliqxError};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn client(server: &MockServer, timeout: Duration) -> Client {
    Client::new(
        format!("{}/", server.uri()),
        ClientOptions {
            timeout,
            ..ClientOptions::default()
        },
    )
    .expect("client")
}

fn envelope(operation: &str, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "api_version": "templiqx/v1alpha1",
        "diagnostics": [],
        "fingerprints": {},
        "ok": true,
        "operation": operation,
        "result": result,
        "stream_events": []
    })
}

#[tokio::test]
async fn encodes_artifact_path_query_and_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operations/v1/artifacts/folder/a%20b.json"))
        .and(query_param("package", "sdk package"))
        .and(query_param("workspace", "review"))
        .and(header("x-request-id", "unit-request"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-request-id", "server-request")
                .set_body_json(envelope(
                    "read_artifact",
                    serde_json::json!({"bytes": "abc"}),
                )),
        )
        .mount(&server)
        .await;

    let response = client(&server, Duration::from_secs(2))
        .read_artifact(
            "folder/a b.json",
            "sdk package",
            Some("review"),
            CallOptions {
                request_id: Some("unit-request".into()),
                ..CallOptions::default()
            },
        )
        .await
        .expect("response");

    assert!(response.data.ok);
    assert_eq!(response.request_id, "server-request");
}

#[tokio::test]
async fn per_call_timeout_maps_to_transport_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operations/v1/catalog"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(100))
                .set_body_json(envelope("catalog", serde_json::json!([]))),
        )
        .mount(&server)
        .await;

    let error = client(&server, Duration::from_secs(2))
        .catalog(CallOptions {
            timeout: Some(Duration::from_millis(5)),
            request_id: Some("timeout-request".into()),
            ..CallOptions::default()
        })
        .await
        .expect_err("request should time out");

    match error {
        TempliqxError::Transport(error) => {
            assert_eq!(error.request_id, "timeout-request");
            assert!(error.is_timeout());
        }
        other => panic!("expected transport error, got {other:?}"),
    }
}

#[tokio::test]
async fn non_success_json_maps_diagnostics_and_text_keeps_raw_body() {
    let server = MockServer::start().await;
    let diagnostics = serde_json::json!({
        "api_version": "templiqx/v1alpha1",
        "diagnostics": [{
            "code": "TQX_NOT_FOUND",
            "message": "not found",
            "severity": "error"
        }],
        "fingerprints": {},
        "ok": false,
        "operation": "catalog",
        "stream_events": []
    });
    Mock::given(method("GET"))
        .and(path("/operations/v1/catalog"))
        .respond_with(ResponseTemplate::new(404).set_body_json(diagnostics))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/operations/v1/catalog"))
        .respond_with(ResponseTemplate::new(502).set_body_string("bad gateway"))
        .mount(&server)
        .await;

    let first = client(&server, Duration::from_secs(2))
        .catalog(CallOptions::default())
        .await
        .expect_err("404");
    match first {
        TempliqxError::Http(error) => {
            assert_eq!(error.status.as_u16(), 404);
            assert_eq!(
                error.envelope.expect("envelope").diagnostics[0].code,
                "TQX_NOT_FOUND"
            );
            assert!(error.raw_body.is_none());
        }
        other => panic!("expected HTTP error, got {other:?}"),
    }

    let second = client(&server, Duration::from_secs(2))
        .catalog(CallOptions::default())
        .await
        .expect_err("502");
    match second {
        TempliqxError::Http(error) => {
            assert_eq!(error.status.as_u16(), 502);
            assert!(error.envelope.is_none());
            assert_eq!(error.raw_body.as_deref(), Some("bad gateway"));
        }
        other => panic!("expected HTTP error, got {other:?}"),
    }
}

#[tokio::test]
async fn default_request_id_is_a_version_four_uuid() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operations/v1/catalog"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(envelope("catalog", serde_json::json!([]))),
        )
        .mount(&server)
        .await;

    let response = client(&server, Duration::from_secs(2))
        .catalog(CallOptions::default())
        .await
        .expect("response");
    let bytes = response.request_id.as_bytes();
    assert_eq!(response.request_id.len(), 36);
    assert_eq!(bytes[14], b'4');
    assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'));
}
