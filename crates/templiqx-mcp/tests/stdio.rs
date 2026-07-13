use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

fn send(stdin: &mut impl Write, message: &Value) -> Result<()> {
    serde_json::to_writer(&mut *stdin, message)?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

fn assert_protocol_line(line: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(line)
        .with_context(|| format!("MCP stdout contained non-JSON: {line:?}"))?;
    ensure!(value["jsonrpc"] == "2.0", "non-JSON-RPC stdout: {value}");
    ensure!(
        value.get("id").is_some() || value.get("method").is_some(),
        "stdout JSON was not a JSON-RPC message: {value}"
    );
    Ok(value)
}

fn response(rx: &Receiver<String>, expected_id: u64, seen: &mut Vec<String>) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(
            !remaining.is_zero(),
            "timed out waiting for MCP response {expected_id}"
        );
        let line = rx
            .recv_timeout(remaining)
            .with_context(|| format!("MCP stdout closed before response {expected_id}"))?;
        let value = assert_protocol_line(&line)?;
        seen.push(line);
        if value["id"] == expected_id {
            return Ok(value);
        }
    }
}

#[test]
fn binary_stdio_is_protocol_clean_and_serves_tools() -> Result<()> {
    let packages = tempfile::tempdir()?;
    templiqx_local::create_package(packages.path(), "demo", "0.1.0")?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_templiqx-mcp"))
        .arg(packages.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn templiqx-mcp")?;
    let mut stdin = child.stdin.take().context("child stdin")?;
    let stdout = child.stdout.take().context("child stdout")?;
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let mut seen = Vec::new();

    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "stdio-conformance", "version": "0.1.0"}
            }
        }),
    )?;
    let initialized = response(&rx, 1, &mut seen)?;
    ensure!(
        initialized.get("result").is_some(),
        "initialize failed: {initialized}"
    );
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )?;

    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )?;
    let listed = response(&rx, 2, &mut seen)?;
    ensure!(
        listed["result"]["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "discover_packages")),
        "tools/list omitted discover_packages: {listed}"
    );

    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":4,"method":"resources/list","params":{}}),
    )?;
    let resources = response(&rx, 4, &mut seen)?;
    ensure!(
        resources["result"]["resources"]
            .as_array()
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item["uri"] == templiqx_mcp::RESOURCE_CATALOG_URI)
                    && items
                        .iter()
                        .any(|item| item["uri"] == templiqx_mcp::RESOURCE_PACKAGES_URI)
            }),
        "resources/list missing templiqx resources: {resources}"
    );

    send(
        &mut stdin,
        &json!({
            "jsonrpc":"2.0",
            "id":5,
            "method":"resources/read",
            "params":{"uri": templiqx_mcp::RESOURCE_CATALOG_URI}
        }),
    )?;
    let catalog = response(&rx, 5, &mut seen)?;
    ensure!(
        catalog["result"]["contents"]
            .as_array()
            .is_some_and(|contents| !contents.is_empty()),
        "resources/read catalog failed: {catalog}"
    );

    send(
        &mut stdin,
        &json!({
            "jsonrpc":"2.0",
            "id":3,
            "method":"tools/call",
            "params":{"name":"discover_packages","arguments":{}}
        }),
    )?;
    let called = response(&rx, 3, &mut seen)?;
    ensure!(
        called["result"]["isError"] != true,
        "tool call failed: {called}"
    );
    ensure!(
        called["result"]["structuredContent"]["operation"] == "discover_packages",
        "unexpected tool response: {called}"
    );

    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            anyhow::bail!("templiqx-mcp did not terminate after stdin EOF");
        }
        thread::sleep(Duration::from_millis(20));
    }
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))?;
    for line in rx.try_iter() {
        assert_protocol_line(&line)?;
        seen.push(line);
    }
    ensure!(!seen.is_empty());
    Ok(())
}
