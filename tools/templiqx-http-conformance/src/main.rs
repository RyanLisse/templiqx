use std::{env, process, time::Duration};
use templiqx_contracts::{CompiledInteraction, CompiledMessage, ExecutionRequest, Role};
use templiqx_ports::{PortError, RuntimeAdapter, RuntimeFailureCode};
use templiqx_runtime_http_mock::HttpMockRuntime;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let url =
        env::var("TEMPLIQX_RUNTIME_URL").unwrap_or_else(|_| "http://mock-gateway:8080".into());
    let scenario =
        env::var("TEMPLIQX_RUNTIME_SCENARIO").unwrap_or_else(|_| "intake-document-01".into());
    let timeout_ms = env::var("TEMPLIQX_RUNTIME_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5_000);
    let max_attempts = env::var("TEMPLIQX_HTTP_CONFORMANCE_MAX_ATTEMPTS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1)
        .max(1);
    let runtime = HttpMockRuntime::new(&url, scenario, Duration::from_millis(timeout_ms))?;
    let request = ExecutionRequest {
        interaction: CompiledInteraction {
            compiler: "templiqx-http-conformance".into(),
            contract_id: "bli-61-date-term-extraction".into(),
            contract_version: "mock".into(),
            messages: vec![CompiledMessage {
                role: Role::User,
                content: "conformance".into(),
            }],
            output_schema: serde_json::json!({"type":"object"}),
            required_capabilities: vec![],
            target_capabilities: vec![],
            runtime_policy: Default::default(),
            extensions: Default::default(),
        },
        fixture_output: Some(serde_json::json!({"ok": true})),
    };

    let mut attempts = 0usize;
    let mut last_failure: Option<(RuntimeFailureCode, String)> = None;
    for _ in 0..max_attempts {
        attempts += 1;
        match runtime.execute(&request) {
            Ok(receipt) => {
                if !receipt.output.is_null() {
                    return Err("HTTP mock receipt carried a payload".into());
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "api_version": "templiqx/http-conformance/v1",
                        "ok": true,
                        "request_fingerprint": receipt.request_fingerprint,
                        "output_fingerprint": receipt.output_fingerprint,
                        "schema_valid": receipt.output_schema_valid,
                        "attempts": attempts,
                    })
                );
                return Ok(());
            }
            Err(PortError::RuntimeFailure { failure, .. }) => {
                let code = failure.code;
                last_failure = Some((code, failure.fingerprint.clone()));
                if is_retryable(code) && attempts < max_attempts {
                    continue;
                }
                let terminal = if is_retryable(code) && attempts >= max_attempts && max_attempts > 1
                {
                    RuntimeFailureCode::HostRetryExhausted
                } else {
                    code
                };
                print_failure(
                    terminal,
                    attempts,
                    last_failure.as_ref().map(|(_, fp)| fp.as_str()),
                );
                process::exit(2);
            }
            Err(error) => return Err(error.to_string().into()),
        }
    }

    let (code, fingerprint) =
        last_failure.unwrap_or((RuntimeFailureCode::Unavailable, String::new()));
    let terminal = if is_retryable(code) && attempts >= max_attempts && max_attempts > 1 {
        RuntimeFailureCode::HostRetryExhausted
    } else {
        code
    };
    print_failure(
        terminal,
        attempts,
        (!fingerprint.is_empty()).then_some(fingerprint.as_str()),
    );
    process::exit(2);
}

fn is_retryable(code: RuntimeFailureCode) -> bool {
    matches!(
        code,
        RuntimeFailureCode::Timeout
            | RuntimeFailureCode::RateLimited
            | RuntimeFailureCode::Unavailable
    )
}

fn print_failure(code: RuntimeFailureCode, attempts: usize, prior_fingerprint: Option<&str>) {
    println!(
        "{}",
        serde_json::json!({
            "api_version": "templiqx/http-conformance/v1",
            "ok": false,
            "code": code.as_str(),
            "attempts": attempts,
            "prior_fingerprint": prior_fingerprint,
        })
    );
}
