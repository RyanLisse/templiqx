use anyhow::{Context, Result, bail, ensure};
use std::{
    env,
    path::{Path, PathBuf},
    process,
    time::Duration,
};
use templiqx_contracts::{CompiledInteraction, CompiledMessage, ExecutionRequest, Role};
use templiqx_mock::{
    ScenarioExpectation, failure_receipt_fingerprint, load_inventory, success_receipt_fingerprint,
};
use templiqx_ports::{PortError, RuntimeAdapter, RuntimeFailureCode};
use templiqx_runtime_http_mock::HttpMockRuntime;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

const RETRY_BACKOFF: Duration = Duration::from_millis(1_000);

fn run() -> Result<()> {
    let url =
        env::var("TEMPLIQX_RUNTIME_URL").unwrap_or_else(|_| "http://mock-gateway:8080".into());
    let scenario =
        env::var("TEMPLIQX_RUNTIME_SCENARIO").unwrap_or_else(|_| "intake-document-01".into());
    let expectation = load_expectation(&scenario)?;
    let timeout_ms = env::var("TEMPLIQX_RUNTIME_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5_000);
    let max_attempts = env::var("TEMPLIQX_HTTP_CONFORMANCE_MAX_ATTEMPTS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1)
        .max(1);
    let runtime = HttpMockRuntime::new(&url, &scenario, Duration::from_millis(timeout_ms))?;
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
                    bail!("HTTP mock receipt carried a payload");
                }
                let (status, code, receipt_fingerprint) = if receipt.output_schema_valid {
                    ("success", None, success_receipt_fingerprint(&receipt))
                } else {
                    let code = "TQX_OUTPUT_SCHEMA";
                    (
                        "failure",
                        Some(code),
                        failure_receipt_fingerprint(code, &receipt.output_fingerprint, None),
                    )
                };
                validate_expectation(
                    &expectation,
                    status,
                    code,
                    Some(receipt.output_schema_valid),
                    Some(&receipt.output_fingerprint),
                    &receipt_fingerprint,
                )?;
                println!(
                    "{}",
                    serde_json::json!({
                        "api_version": "templiqx/http-conformance/v1",
                        "ok": true,
                        "scenario_id": scenario,
                        "status": status,
                        "code": code,
                        "request_fingerprint": receipt.request_fingerprint,
                        "output_fingerprint": receipt.output_fingerprint,
                        "schema_valid": receipt.output_schema_valid,
                        "receipt_fingerprint": receipt_fingerprint,
                        "attempts": attempts,
                    })
                );
                return Ok(());
            }
            Err(PortError::RuntimeFailure { failure, .. }) => {
                let code = failure.code;
                let receipt_fingerprint = failure_receipt_fingerprint(
                    code.as_str(),
                    &failure.fingerprint,
                    failure.retry_after_ms,
                );
                if expectation.diagnostic_code.as_deref() == Some(code.as_str()) {
                    validate_expectation(
                        &expectation,
                        "failure",
                        Some(code.as_str()),
                        None,
                        None,
                        &receipt_fingerprint,
                    )?;
                    println!(
                        "{}",
                        serde_json::json!({
                            "api_version": "templiqx/http-conformance/v1", "ok": true,
                            "scenario_id": scenario, "status": "failure", "code": code.as_str(),
                            "schema_valid": null, "receipt_fingerprint": receipt_fingerprint,
                            "attempts": attempts,
                        })
                    );
                    return Ok(());
                }
                last_failure = Some((code, failure.fingerprint.clone()));
                if is_retryable(code) && attempts < max_attempts {
                    // Connection failures (e.g. a not-yet-ready gateway) fail
                    // instantly, so without a delay the whole attempt budget
                    // burns through in milliseconds and never gives the host
                    // a real window to become reachable.
                    std::thread::sleep(RETRY_BACKOFF);
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
            Err(error) => return Err(anyhow::anyhow!(error.to_string())),
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

fn scenario_root() -> PathBuf {
    env::var_os("TEMPLIQX_MOCK_SCENARIO_ROOT")
        .map(PathBuf::from)
        .or_else(|| {
            Path::new("/packages/crm3/scenarios")
                .is_dir()
                .then(|| PathBuf::from("/packages/crm3/scenarios"))
        })
        .unwrap_or_else(|| PathBuf::from("examples/crm3/scenarios"))
}

fn load_expectation(scenario: &str) -> Result<ScenarioExpectation> {
    let inventory = load_inventory(scenario_root().join("inventory.json"), "crm3")
        .context("load mock scenario inventory")?;
    inventory
        .scenarios
        .into_iter()
        .find(|entry| entry.id == scenario)
        .map(|entry| entry.expectation)
        .with_context(|| format!("scenario '{scenario}' is not registered in inventory"))
}

fn validate_expectation(
    expected: &ScenarioExpectation,
    status: &str,
    code: Option<&str>,
    schema_valid: Option<bool>,
    output_fingerprint: Option<&str>,
    receipt_fingerprint: &str,
) -> Result<()> {
    ensure!(
        expected.status == status,
        "expected status {}, got {status}",
        expected.status
    );
    ensure!(
        expected.diagnostic_code.as_deref() == code,
        "expected diagnostic {:?}, got {code:?}",
        expected.diagnostic_code
    );
    ensure!(
        expected.output_schema_valid == schema_valid,
        "expected schema validity {:?}, got {schema_valid:?}",
        expected.output_schema_valid
    );
    if let Some(fingerprint) = expected.output_fingerprint.as_deref() {
        ensure!(
            output_fingerprint == Some(fingerprint),
            "output fingerprint does not match inventory"
        );
    }
    ensure!(
        expected.receipt_fingerprint == receipt_fingerprint,
        "receipt fingerprint does not match inventory"
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incorrect_expectations_fail_closed() {
        let expected = ScenarioExpectation {
            status: "success".into(),
            diagnostic_code: None,
            output_schema_valid: Some(true),
            output_fingerprint: Some("output".into()),
            receipt_fingerprint: "receipt".into(),
        };
        assert!(
            validate_expectation(
                &expected,
                "failure",
                Some("TQX_OUTPUT_SCHEMA"),
                Some(false),
                Some("other"),
                "other"
            )
            .is_err()
        );
        assert!(
            validate_expectation(
                &expected,
                "success",
                None,
                Some(true),
                Some("output"),
                "receipt"
            )
            .is_ok()
        );
    }
}
