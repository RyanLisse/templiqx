use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc::{self, Receiver},
    thread,
};
use templiqx_contracts::{CompiledInteraction, CompiledMessage, ExecutionRequest, Role};

fn model() -> ModelConfig {
    ModelConfig {
        base_url: "https://api.openai.com/v1".into(),
        api_key: "sk-test".into(),
        model: "gpt-5.4-mini".into(),
        timeout: Duration::from_secs(5),
    }
}

fn langfuse() -> LangfuseConfig {
    LangfuseConfig {
        host: "https://cloud.langfuse.com".into(),
        public_key: "pk-test".into(),
        secret_key: "sk-test".into(),
    }
}

fn request() -> ExecutionRequest {
    ExecutionRequest {
        interaction: CompiledInteraction {
            compiler: "test".into(),
            contract_id: "contract".into(),
            contract_version: "1".into(),
            messages: vec![CompiledMessage {
                role: Role::User,
                content: "hello".into(),
            }],
            output_schema: json!({"type":"object","required":["ok"],"properties":{"ok":{"type":"boolean"}}}),
            required_capabilities: vec![],
            target_capabilities: vec![],
            runtime_policy: Default::default(),
            extensions: Default::default(),
        },
        fixture_output: None,
    }
}

fn loopback(response: Vec<u8>, delay: Duration) -> (String, Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut bytes = Vec::new();
        let header_end = loop {
            let mut chunk = [0; 1024];
            let read = stream.read(&mut chunk).unwrap_or(0);
            if read == 0 {
                break bytes.len();
            }
            bytes.extend_from_slice(&chunk[..read]);
            if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let content_length = String::from_utf8_lossy(&bytes[..header_end])
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let mut chunk = [0; 4096];
            let read = stream.read(&mut chunk).unwrap_or(0);
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        tx.send(bytes).ok();
        thread::sleep(delay);
        stream.write_all(&response).ok();
    });
    (format!("http://{address}/v1"), rx)
}

fn http_json(status: u16, reason: &str, value: &Value, extra_headers: &str) -> Vec<u8> {
    let body = serde_json::to_vec(value).unwrap();
    format!("HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n{extra_headers}connection: close\r\n\r\n", body.len()).bytes().chain(body).collect()
}

fn runtime(model_url: String, trace_url: String, timeout: Duration) -> LangfuseTracedRuntime {
    LangfuseTracedRuntime::new(
        ModelConfig {
            base_url: model_url,
            api_key: "model-secret-token".into(),
            model: "test-model".into(),
            timeout,
        },
        LangfuseConfig {
            host: trace_url,
            public_key: "trace-public-key".into(),
            secret_key: "trace-secret-key".into(),
        },
    )
    .unwrap()
}

#[test]
fn base64_encode_matches_known_vectors() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64_encode(b"pk-test:sk-test"), "cGstdGVzdDpzay10ZXN0");
}

#[test]
fn basic_auth_header_is_base64_of_public_colon_secret() {
    assert_eq!(
        basic_auth("pk-test", "sk-test"),
        "Basic cGstdGVzdDpzay10ZXN0"
    );
}

#[test]
fn new_rejects_empty_fields() {
    let mut cfg = model();
    cfg.api_key = "  ".into();
    let error = LangfuseTracedRuntime::new(cfg, langfuse()).unwrap_err();
    assert!(matches!(error, PortError::InvalidData(_)));
}

#[test]
fn new_succeeds_with_valid_config() {
    let runtime = LangfuseTracedRuntime::new(model(), langfuse()).unwrap();
    let descriptor = runtime.descriptor();
    assert_eq!(descriptor.id, "templiqx-runtime-langfuse");
    assert!(
        descriptor
            .capabilities
            .contains(&"chat_completion".to_string())
    );
}

#[test]
fn map_ureq_status_error_uses_rate_limited_for_429() {
    let runtime = LangfuseTracedRuntime::new(model(), langfuse()).unwrap();
    let response = ureq::Response::new(429, "Too Many Requests", "{}").unwrap();
    let error = runtime.map_ureq_error(ureq::Error::Status(429, response));
    match error {
        PortError::RuntimeFailure { failure, .. } => {
            assert_eq!(failure.code, RuntimeFailureCode::RateLimited);
        }
        other => panic!("expected runtime failure, got {other:?}"),
    }
}

#[test]
fn loopback_maps_structured_request_and_schema_outcome() {
    let completion = json!({"choices":[{"message":{"content":"{\"ok\":true}"}}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}});
    let (model_url, model_request) =
        loopback(http_json(200, "OK", &completion, ""), Duration::ZERO);
    let (trace_url, _trace_request) = loopback(
        http_json(503, "Unavailable", &json!({}), ""),
        Duration::ZERO,
    );
    let receipt = runtime(model_url, trace_url, Duration::from_secs(2))
        .execute(&request())
        .unwrap();
    assert!(receipt.output_schema_valid);
    assert_eq!(receipt.output, json!({"ok":true}));

    let wire = model_request.recv_timeout(Duration::from_secs(2)).unwrap();
    let separator = wire
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap()
        + 4;
    let json: Value = serde_json::from_slice(&wire[separator..]).unwrap();
    assert_eq!(json["model"], "test-model");
    assert_eq!(json["messages"][0]["content"], "hello");
    assert_eq!(json["response_format"]["type"], "json_schema");
    assert_eq!(json["response_format"]["json_schema"]["strict"], true);
    assert_eq!(
        json["response_format"]["json_schema"]["schema"],
        request().interaction.output_schema
    );

    let invalid = json!({"choices":[{"message":{"content":"{\"ok\":\"yes\"}"}}],"usage":null});
    let (model_url, _) = loopback(http_json(200, "OK", &invalid, ""), Duration::ZERO);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let trace_url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let receipt = runtime(model_url, trace_url, Duration::from_millis(50))
        .execute(&request())
        .unwrap();
    assert!(!receipt.output_schema_valid);
}

#[test]
fn loopback_maps_429_retry_after_and_5xx_without_response_body_leaks() {
    for (status, reason, headers, expected, retry) in [
        (
            429,
            "Too Many Requests",
            "retry-after: 3\r\n",
            RuntimeFailureCode::RateLimited,
            Some(3000),
        ),
        (
            503,
            "Unavailable",
            "",
            RuntimeFailureCode::Unavailable,
            None,
        ),
    ] {
        let secret_body = json!({"error":"model-secret-token trace-secret-key"});
        let (model_url, _) = loopback(
            http_json(status, reason, &secret_body, headers),
            Duration::ZERO,
        );
        let adapter = runtime(
            model_url,
            "http://127.0.0.1:9".into(),
            Duration::from_secs(2),
        );
        match adapter.execute(&request()).unwrap_err() {
            PortError::RuntimeFailure { failure, .. } => {
                assert_eq!(failure.code, expected);
                assert_eq!(failure.retry_after_ms, retry);
                assert!(!failure.detail.contains("model-secret-token"));
                assert!(!failure.detail.contains("trace-secret-key"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

#[test]
fn loopback_maps_timeout_malformed_and_oversized_responses() {
    let cases = [
        (
            b"HTTP/1.1 200 OK\r\ncontent-length: 1\r\nconnection: close\r\n\r\n{".to_vec(),
            Duration::ZERO,
            RuntimeFailureCode::InvalidResponse,
        ),
        (
            http_json(200, "OK", &json!({"choices":[]}), ""),
            Duration::ZERO,
            RuntimeFailureCode::InvalidResponse,
        ),
        (
            http_json(
                200,
                "OK",
                &json!({"choices":[{"message":{"content":"not-json"}}]}),
                "",
            ),
            Duration::ZERO,
            RuntimeFailureCode::InvalidResponse,
        ),
        (
            b"HTTP/1.1 200 OK\r\ncontent-length: 2097153\r\nconnection: close\r\n\r\n"
                .iter()
                .copied()
                .chain(std::iter::repeat_n(b'x', 2_097_153))
                .collect(),
            Duration::ZERO,
            RuntimeFailureCode::InvalidResponse,
        ),
        (
            http_json(200, "OK", &json!({}), ""),
            Duration::from_millis(150),
            RuntimeFailureCode::Timeout,
        ),
    ];
    for (response, delay, expected) in cases {
        let (model_url, _) = loopback(response, delay);
        // Oversized bodies need headroom under load; keep a tight budget only for the
        // intentional timeout case (delay 150ms vs client 25ms).
        let client_timeout = match expected {
            RuntimeFailureCode::Timeout => Duration::from_millis(25),
            _ => Duration::from_secs(2),
        };
        let adapter = runtime(model_url, "http://127.0.0.1:9".into(), client_timeout);
        match adapter.execute(&request()).unwrap_err() {
            PortError::RuntimeFailure { failure, .. } => assert_eq!(failure.code, expected),
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

#[test]
fn trace_endpoint_failure_is_non_fatal_and_default_streaming_has_terminal_parity() {
    let completion = json!({"choices":[{"message":{"content":"{\"ok\":true}"}}],"usage":null});
    let (model_url, _) = loopback(http_json(200, "OK", &completion, ""), Duration::ZERO);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let trace_url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let adapter = runtime(model_url, trace_url, Duration::from_millis(50));
    let mut events = Vec::new();
    let receipt = adapter
        .execute_streaming(&request(), &mut |event| events.push(event))
        .unwrap();
    assert!(receipt.output_schema_valid);
    assert!(
        matches!(events.as_slice(), [templiqx_contracts::StreamEvent::Complete(complete)] if complete == &receipt)
    );
}

#[test]
fn map_ureq_status_error_uses_unavailable_for_5xx() {
    let runtime = LangfuseTracedRuntime::new(model(), langfuse()).unwrap();
    let response = ureq::Response::new(503, "Service Unavailable", "{}").unwrap();
    let error = runtime.map_ureq_error(ureq::Error::Status(503, response));
    match error {
        PortError::RuntimeFailure { failure, .. } => {
            assert_eq!(failure.code, RuntimeFailureCode::Unavailable);
        }
        other => panic!("expected runtime failure, got {other:?}"),
    }
}
