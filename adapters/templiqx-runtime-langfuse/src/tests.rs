use super::*;

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
