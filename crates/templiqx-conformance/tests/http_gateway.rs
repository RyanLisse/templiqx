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
use templiqx_ports::{RuntimeAdapter, RuntimeFailureCode};
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
    if BUILT.get().is_none() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let status = Command::new(&cargo)
            .current_dir(&repo)
            .args(["build", "--quiet", "-p", "templiqx-mock-gateway"])
            .status()?;
        ensure!(status.success(), "failed to build templiqx-mock-gateway");
        let _ = BUILT.set(repo.join("target/debug/templiqx-mock-gateway"));
    }
    Ok(BUILT.get().expect("mock gateway binary").clone())
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
