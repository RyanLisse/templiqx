use anyhow::{Context, Result, ensure};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};
use templiqx_contracts::{CompiledInteraction, CompiledMessage, ExecutionRequest, Role};
use templiqx_mock::{failure_receipt_fingerprint, load_inventory, success_receipt_fingerprint};
use templiqx_ports::{PortError, RuntimeAdapter, RuntimeFailureCode};
use templiqx_runtime_http_mock::HttpMockRuntime;

fn request() -> ExecutionRequest {
    ExecutionRequest {
        interaction: CompiledInteraction {
            compiler: "conformance".into(),
            contract_id: "bli-61-date-term-extraction".into(),
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

struct Gateway(Child);
impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn mock_gateway_binary() -> Result<PathBuf> {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    if let Some(path) = BUILT.get() {
        return Ok(path.clone());
    }
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let binary = repo.join("target/debug/templiqx-mock-gateway");
    if !binary.is_file() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let status = Command::new(&cargo)
            .current_dir(&repo)
            .args(["build", "--quiet", "-p", "templiqx-mock-gateway"])
            .status()?;
        ensure!(status.success(), "failed to build templiqx-mock-gateway");
    }
    Ok(BUILT.get_or_init(|| binary).clone())
}

fn gateway() -> Result<(Gateway, String)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scenario_root = root.join("examples/crm3/scenarios");
    let child = Command::new(mock_gateway_binary()?)
        .args([
            "--listen",
            &address.to_string(),
            "--scenario-root",
            scenario_root.to_str().context("UTF-8 scenario root")?,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if TcpStream::connect(address).is_ok() {
            return Ok((Gateway(child), format!("http://{address}")));
        }
        thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!("mock gateway did not start")
}

fn raw_status(url: &str, request: &str) -> Result<u16> {
    let mut stream = TcpStream::connect(url.strip_prefix("http://").context("HTTP URL")?)?;
    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    std::io::Read::read_to_string(&mut stream, &mut response)?;
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .context("HTTP response status")
}

#[test]
fn real_gateway_transport_asserts_success_and_runtime_failure() -> Result<()> {
    let (_gateway, url) = gateway()?;
    let success = HttpMockRuntime::with_default_timeout(&url, "intake-document-01")?;
    let receipt = success.execute(&request())?;
    ensure!(
        receipt.output.is_null(),
        "gateway protocol returned a payload"
    );
    ensure!(receipt.output_schema_valid);

    let failure = HttpMockRuntime::with_default_timeout(&url, "missing-notice-date")?;
    let error = failure.execute(&request()).unwrap_err();
    ensure!(
        error
            .to_string()
            .contains(RuntimeFailureCode::InvalidResponse.as_str())
    );
    Ok(())
}

#[test]
fn every_inventory_scenario_matches_its_expectation_over_real_http() -> Result<()> {
    let (_gateway, url) = gateway()?;
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/crm3/scenarios");
    let inventory = load_inventory(root.join("inventory.json"), "crm3")?;
    ensure!(
        inventory.scenarios.len() == 8,
        "CRM3 HTTP matrix must contain exactly eight scenarios"
    );

    for entry in inventory.scenarios {
        let runtime = HttpMockRuntime::with_default_timeout(&url, &entry.id)?;
        match runtime.execute(&request()) {
            Ok(receipt) => {
                ensure!(
                    receipt.output.is_null(),
                    "{} returned payload bytes",
                    entry.id
                );
                let (status, code, fingerprint) = if receipt.output_schema_valid {
                    ("success", None, success_receipt_fingerprint(&receipt))
                } else {
                    let code = "TQX_OUTPUT_SCHEMA";
                    (
                        "failure",
                        Some(code),
                        failure_receipt_fingerprint(code, &receipt.output_fingerprint, None),
                    )
                };
                ensure!(
                    entry.expectation.status == status,
                    "{} status mismatch",
                    entry.id
                );
                ensure!(
                    entry.expectation.diagnostic_code.as_deref() == code,
                    "{} diagnostic mismatch",
                    entry.id
                );
                ensure!(
                    entry.expectation.output_schema_valid == Some(receipt.output_schema_valid),
                    "{} schema expectation mismatch",
                    entry.id
                );
                if let Some(expected) = entry.expectation.output_fingerprint.as_deref() {
                    ensure!(
                        receipt.output_fingerprint == expected,
                        "{} output fingerprint mismatch",
                        entry.id
                    );
                }
                ensure!(
                    entry.expectation.receipt_fingerprint == fingerprint,
                    "{} receipt fingerprint mismatch",
                    entry.id
                );
            }
            Err(PortError::RuntimeFailure { failure, .. }) => {
                let fingerprint = failure_receipt_fingerprint(
                    failure.code.as_str(),
                    &failure.fingerprint,
                    failure.retry_after_ms,
                );
                ensure!(
                    entry.expectation.status == "failure",
                    "{} unexpectedly failed",
                    entry.id
                );
                ensure!(
                    entry.expectation.diagnostic_code.as_deref() == Some(failure.code.as_str()),
                    "{} failure code mismatch",
                    entry.id
                );
                ensure!(
                    entry.expectation.output_schema_valid.is_none(),
                    "{} failure declared schema output",
                    entry.id
                );
                ensure!(
                    entry.expectation.receipt_fingerprint == fingerprint,
                    "{} failure receipt fingerprint mismatch",
                    entry.id
                );
            }
            Err(error) => anyhow::bail!("{} returned non-runtime error: {error}", entry.id),
        }
    }
    Ok(())
}

#[test]
fn gateway_rejects_unlisted_malformed_and_oversized_requests() -> Result<()> {
    let (_gateway, url) = gateway()?;
    ensure!(
        raw_status(
            &url,
            "POST /v1/scenarios/not-in-inventory/execute HTTP/1.1\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}"
        )? == 404
    );
    ensure!(
        raw_status(
            &url,
            "POST /v1/scenarios/intake-document-01/execute HTTP/1.1\r\ncontent-length: 1\r\nconnection: close\r\n\r\n{"
        )? == 400
    );
    ensure!(
        raw_status(
            &url,
            "POST /v1/scenarios/intake-document-01/execute HTTP/1.1\r\ncontent-length: 1048577\r\nconnection: close\r\n\r\n"
        )? == 413
    );
    ensure!(
        raw_status(
            &url,
            "PUT /v1/scenarios/intake-document-01/execute HTTP/1.1\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        )? == 404
    );
    Ok(())
}

#[test]
fn real_http_transport_asserts_unavailable_and_timeout() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let unavailable =
        HttpMockRuntime::with_default_timeout(&format!("http://{address}"), "intake-document-01")?;
    ensure!(
        unavailable
            .execute(&request())
            .unwrap_err()
            .to_string()
            .contains(RuntimeFailureCode::Unavailable.as_str())
    );

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request);
            thread::sleep(Duration::from_millis(150));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}");
        }
    });
    let timeout = HttpMockRuntime::new(
        &format!("http://{address}"),
        "intake-document-01",
        Duration::from_millis(20),
    )?;
    let timeout_error = timeout.execute(&request()).unwrap_err();
    ensure!(
        timeout_error
            .to_string()
            .contains(RuntimeFailureCode::Timeout.as_str()),
        "unexpected timeout error: {timeout_error}"
    );
    Ok(())
}
