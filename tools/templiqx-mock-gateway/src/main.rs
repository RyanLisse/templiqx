//! Conformance-only HTTP transport for the deterministic scenario runtime.
//! This binary is deliberately outside the Templiqx production workspace graph.

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
};
use templiqx_contracts::{CompiledInteraction, ExecutionRequest};
use templiqx_mock::{ScenarioManifest, ScriptedRuntime, load_inventory, scenario_fingerprint};
use templiqx_ports::{PortError, RuntimeAdapter};

const API_VERSION: &str = "templiqx.mock/v1alpha1";

#[derive(Debug, Parser)]
#[command(
    name = "templiqx-mock-gateway",
    about = "Conformance-only mock scenario gateway"
)]
struct Args {
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: String,
    #[arg(long)]
    scenario_root: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Outcome {
    Success {
        request_fingerprint: String,
        output_fingerprint: String,
        output_schema_valid: bool,
    },
    Failure {
        code: String,
        detail: String,
        retry_after_ms: Option<u64>,
        failure_fingerprint: String,
    },
}

#[derive(Debug, Serialize)]
struct ScenarioResponse {
    api_version: &'static str,
    scenario_id: String,
    contract: String,
    scenario_fingerprint: String,
    attempts: usize,
    elapsed_ms: u64,
    outcome: Outcome,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = args
        .scenario_root
        .or_else(|| std::env::var_os("TEMPLIQX_MOCK_SCENARIO_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("examples/crm3/scenarios"));
    let root = root
        .canonicalize()
        .with_context(|| format!("canonicalize scenario root {}", root.display()))?;
    let listener =
        TcpListener::bind(&args.listen).with_context(|| format!("bind {}", args.listen))?;
    eprintln!(
        "templiqx-mock-gateway listening on {} scenario_root={}",
        args.listen,
        root.display()
    );
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                // This is a single-threaded, one-connection-at-a-time server: a
                // client that connects and never finishes sending a request would
                // otherwise block `read()` forever, wedging every later
                // connection (including conformance-job retries) behind it.
                let deadline = std::time::Duration::from_secs(10);
                if let Err(error) = stream
                    .set_read_timeout(Some(deadline))
                    .and_then(|_| stream.set_write_timeout(Some(deadline)))
                {
                    eprintln!("set stream timeout: {error}");
                    continue;
                }
                if let Err(error) = handle(&mut stream, &root) {
                    let _ = respond(&mut stream, 500, &json!({"error": error.to_string()}));
                }
            }
            Err(error) => eprintln!("accept: {error}"),
        }
    }
    Ok(())
}

fn handle(stream: &mut TcpStream, root: &Path) -> Result<()> {
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut chunk = [0_u8; 1024];
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break None;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break Some(end + 4);
        }
        if bytes.len() > 64 * 1024 {
            bail!("request headers too large");
        }
    }
    .context("incomplete HTTP request")?;
    let head = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
    let content_length = head
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-length:")
                .or_else(|| line.strip_prefix("Content-Length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let mut chunk = [0_u8; 4096];
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            bail!("request body shorter than content-length");
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body_bytes = &bytes[header_end..header_end + content_length];
    let body = String::from_utf8_lossy(body_bytes);
    let mut parts = head.lines().next().unwrap_or_default().split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();

    match (method, path) {
        ("GET", "/health/live") | ("GET", "/health/ready") => {
            respond(stream, 200, &json!({"ok": true}))
        }
        ("GET", "/v1/scenarios") => respond(
            stream,
            200,
            &json!({"api_version": API_VERSION, "scenarios": inventory(root)?}),
        ),
        ("POST", path) => execute(stream, root, path, &body),
        _ => respond(stream, 404, &json!({"error": "not found"})),
    }
}

fn inventory(root: &Path) -> Result<Vec<String>> {
    let inventory = load_inventory(root.join("inventory.json"), "crm3")?;
    let mut ids: Vec<_> = inventory
        .scenarios
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    ids.sort();
    Ok(ids)
}

fn execute(stream: &mut TcpStream, root: &Path, path: &str, body: &str) -> Result<()> {
    let prefix = "/v1/scenarios/";
    let id = path
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix("/execute"));
    let Some(id) = id else {
        return respond(stream, 404, &json!({"error": "not found"}));
    };
    if id.is_empty() || id.contains('/') || id.contains('\\') || id == "." || id == ".." {
        return respond(stream, 400, &json!({"error": "invalid scenario id"}));
    }
    let manifest_path = root.join(id).join("manifest.json");
    let manifest = match ScenarioManifest::load(&manifest_path) {
        Ok(manifest) => manifest,
        Err(_) => return respond(stream, 404, &json!({"error": "scenario not found"})),
    };
    let request_body: Value = if body.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(body).context("decode request JSON")?
    };
    let output = match manifest.expected_output.as_deref() {
        Some(relative) => Some(read_relative_json(root, &manifest_path, relative)?),
        None => request_body.get("fixture_output").cloned(),
    };
    let interaction = CompiledInteraction {
        compiler: "templiqx-mock-gateway".into(),
        contract_id: manifest.contract.clone(),
        contract_version: "mock".into(),
        messages: Vec::new(),
        output_schema: json!({}),
        required_capabilities: Vec::new(),
        target_capabilities: Vec::new(),
        runtime_policy: Default::default(),
        extensions: Default::default(),
    };
    let runtime = ScriptedRuntime::from_manifest(manifest.clone());
    let result = runtime.execute(&ExecutionRequest {
        interaction,
        fixture_output: output,
    });
    let stats = runtime.stats();
    let outcome = match result {
        Ok(receipt) => Outcome::Success {
            request_fingerprint: receipt.request_fingerprint,
            output_fingerprint: receipt.output_fingerprint,
            output_schema_valid: receipt.output_schema_valid,
        },
        Err(PortError::RuntimeFailure {
            code,
            detail,
            failure,
        }) => Outcome::Failure {
            code: code.to_string(),
            detail,
            retry_after_ms: failure.retry_after_ms,
            failure_fingerprint: failure.fingerprint,
        },
        Err(error) => bail!("scenario execution failed: {error}"),
    };
    let scenario_fingerprint = scenario_fingerprint(&manifest);
    let response = ScenarioResponse {
        api_version: API_VERSION,
        scenario_id: manifest.id.clone(),
        contract: manifest.contract,
        scenario_fingerprint,
        attempts: stats.attempts,
        elapsed_ms: stats.elapsed_ms,
        outcome,
    };
    respond(stream, 200, &response)
}

fn read_relative_json(scenario_root: &Path, manifest_path: &Path, relative: &str) -> Result<Value> {
    if Path::new(relative).is_absolute() || relative.contains('\\') {
        bail!("scenario artifact path must be relative");
    }
    let root = manifest_path.parent().context("manifest parent")?;
    let path = root.join(relative);
    let canonical = path
        .canonicalize()
        .with_context(|| format!("read scenario artifact {}", path.display()))?;
    let allowed_root = scenario_root.parent().unwrap_or(scenario_root);
    if !canonical.starts_with(allowed_root) {
        bail!("scenario artifact escapes scenario directory");
    }
    Ok(serde_json::from_slice(&fs::read(canonical)?)?)
}

fn respond<T: Serialize>(stream: &mut TcpStream, status: u16, value: &T) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    Ok(())
}
