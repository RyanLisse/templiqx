use super::*;
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener},
    thread,
};
use templiqx_contracts::{CompiledInteraction, CompiledMessage, Role};

fn request() -> ExecutionRequest {
    ExecutionRequest {
        interaction: CompiledInteraction {
            compiler: "test".into(),
            contract_id: "test-contract".into(),
            contract_version: "1.0.0".into(),
            messages: vec![CompiledMessage {
                role: Role::User,
                content: "hello".into(),
            }],
            output_schema: serde_json::json!({"type":"object"}),
            required_capabilities: vec![],
            target_capabilities: vec![],
            runtime_policy: Default::default(),
            extensions: Default::default(),
        },
        fixture_output: Some(serde_json::json!({"ok": true})),
    }
}

fn serve(status: &str, body: &str) -> String {
    serve_with_headers(status, "", body)
}

fn serve_with_headers(status: &str, headers: &str, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let status = status.to_owned();
    let response_headers = headers.to_owned();
    let body = body.to_owned();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let header_end = loop {
            let mut chunk = [0; 4096];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "request ended before headers were complete");
            request.extend_from_slice(&chunk[..count]);
            if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break header_end + 4;
            }
        };
        let request_headers = String::from_utf8_lossy(&request[..header_end]).to_string();
        let content_length = request_headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length:")
                    .or_else(|| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let mut chunk = [0; 4096];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "request ended before body was complete");
            request.extend_from_slice(&chunk[..count]);
        }
        assert!(
            String::from_utf8_lossy(&request[..header_end])
                .contains("POST /v1/scenarios/intake-document-01/execute HTTP/1.1")
        );
        let response = format!(
            "HTTP/1.1 {status}\r\n{response_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
    });
    format!("http://{address}")
}

#[test]
fn invokes_scenario_endpoint_and_maps_payload_free_outcome() {
    let response = serde_json::json!({
        "api_version": "templiqx.mock/v1alpha1",
        "scenario_id": "intake-document-01",
        "contract": "bli-61-date-term-extraction",
        "scenario_fingerprint": "scenario",
        "attempts": 1,
        "elapsed_ms": 25,
        "outcome": {"type": "success", "request_fingerprint": "request", "output_fingerprint": "output", "output_schema_valid": true}
    });
    let adapter = HttpMockRuntime::with_default_timeout(
        &serve("200 OK", &response.to_string()),
        "intake-document-01",
    )
    .unwrap();
    let receipt = adapter.execute(&request()).unwrap();
    assert_eq!(receipt.request_fingerprint, "request");
    assert_eq!(receipt.output_fingerprint, "output");
    assert!(receipt.output.is_null());
}

#[test]
fn maps_503_to_unavailable() {
    let adapter = HttpMockRuntime::with_default_timeout(
        &serve("503 Service Unavailable", "{}"),
        "intake-document-01",
    )
    .unwrap();
    let error = adapter.execute(&request()).unwrap_err();
    assert!(error.to_string().contains("TQX_RUNTIME_UNAVAILABLE"));
}

#[test]
fn maps_http_statuses_without_retrying() {
    for (status, expected) in [
        ("502 Bad Gateway", RuntimeFailureCode::Unavailable),
        ("503 Service Unavailable", RuntimeFailureCode::Unavailable),
        ("408 Request Timeout", RuntimeFailureCode::Timeout),
        ("504 Gateway Timeout", RuntimeFailureCode::Timeout),
    ] {
        let adapter =
            HttpMockRuntime::with_default_timeout(&serve(status, "{}"), "intake-document-01")
                .unwrap();
        let error = adapter.execute(&request()).unwrap_err();
        assert!(error.to_string().contains(expected.as_str()));
    }
}

#[test]
fn maps_rate_limit_and_normalizes_retry_hint() {
    let adapter = HttpMockRuntime::with_default_timeout(
        &serve_with_headers("429 Too Many Requests", "Retry-After: 3\r\n", "{}"),
        "intake-document-01",
    )
    .unwrap();
    let error = adapter.execute(&request()).unwrap_err();
    match error {
        PortError::RuntimeFailure { failure, .. } => {
            assert_eq!(failure.code, RuntimeFailureCode::RateLimited);
            assert_eq!(failure.retry_after_ms, Some(3_000));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn maps_malformed_success_to_invalid_response() {
    let adapter =
        HttpMockRuntime::with_default_timeout(&serve("200 OK", "not-json"), "intake-document-01")
            .unwrap();
    let error = adapter.execute(&request()).unwrap_err();
    assert!(error.to_string().contains("TQX_RUNTIME_INVALID_RESPONSE"));
}

#[test]
fn maps_connection_failure_to_unavailable_without_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let adapter =
        HttpMockRuntime::with_default_timeout(&format!("http://{address}"), "intake-document-01")
            .unwrap();
    let error = adapter.execute(&request()).unwrap_err();
    assert!(error.to_string().contains("TQX_RUNTIME_UNAVAILABLE"));
}

#[test]
fn maps_request_timeout_to_timeout() {
    let adapter =
        HttpMockRuntime::with_default_timeout("http://127.0.0.1:1", "intake-document-01").unwrap();
    let error = adapter.map_io(std::io::Error::from(std::io::ErrorKind::TimedOut));
    assert!(error.to_string().contains("TQX_RUNTIME_TIMEOUT"));
}
