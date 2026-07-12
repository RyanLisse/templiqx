//! Agent-native parity conformance (plan 002).

use anyhow::{Context, Result, anyhow, ensure};
use rmcp::{
    ServiceExt as _,
    model::{CallToolRequestParams, JsonObject},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use templiqx_application::{CreatePackageRequest, DeleteContractRequest};
use templiqx_contracts::OperationEnvelope;
use templiqx_mcp::TempliqxMcp;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn result<T>(envelope: OperationEnvelope<T>) -> Result<T> {
    ensure!(
        envelope.ok,
        "{} failed: {:?}",
        envelope.operation,
        envelope.diagnostics
    );
    envelope
        .result
        .ok_or_else(|| anyhow!("{} returned no result", envelope.operation))
}

fn cli_envelope(root: &Path, args: &[&str]) -> Result<Value> {
    static BUILT: OnceLock<()> = OnceLock::new();
    let repo = repo_root();
    if BUILT.get().is_none() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let build = Command::new(&cargo)
            .current_dir(&repo)
            .args(["build", "--quiet", "-p", "templiqx-cli"])
            .status()?;
        ensure!(build.success(), "failed to build templiqx CLI");
        let _ = BUILT.set(());
    }
    let binary = repo.join("target/debug").join(if cfg!(windows) {
        "templiqx.exe"
    } else {
        "templiqx"
    });
    let output = Command::new(binary)
        .arg("--root")
        .arg(root)
        .arg("--json")
        .args(args)
        .output()?;
    serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "CLI printed no envelope (status {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn arguments(value: Value) -> JsonObject {
    serde_json::from_value(value).expect("tool arguments are an object")
}

async fn mcp_call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    tool: &str,
    args: Value,
) -> Result<Value> {
    client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments(args)))
        .await?
        .structured_content
        .with_context(|| format!("MCP {tool} structured content"))
}

fn normalize_optional_nulls(value: &mut Value) {
    const OPTIONAL_KEYS: &[&str] = &["result", "file", "json_pointer", "span", "help"];
    match value {
        Value::Object(map) => {
            for key in OPTIONAL_KEYS {
                map.entry(*key).or_insert(Value::Null);
            }
            for v in map.values_mut() {
                normalize_optional_nulls(v);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_optional_nulls),
        _ => {}
    }
}

fn assert_equal_envelopes(rust: &impl Serialize, cli: &Value, mcp: &Value) -> Result<()> {
    let mut rust = serde_json::to_value(rust)?;
    let mut cli = cli.clone();
    let mut mcp = mcp.clone();
    normalize_optional_nulls(&mut rust);
    normalize_optional_nulls(&mut cli);
    normalize_optional_nulls(&mut mcp);
    ensure!(rust == cli, "Rust/CLI mismatch\nRust: {rust}\nCLI: {cli}");
    ensure!(rust == mcp, "Rust/MCP mismatch\nRust: {rust}\nMCP: {mcp}");
    Ok(())
}

#[test]
fn create_package_happy_path_bootstraps_package_and_is_discoverable() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let service = templiqx_local::compose(temp.path())?;

    let manifest = result(service.create_package(&CreatePackageRequest {
        name: "demo".into(),
        version: "0.1.0".into(),
    }))?;
    ensure!(manifest.package == "demo");
    ensure!(manifest.contracts.is_empty());
    ensure!(temp.path().join("demo/templiqx.yaml").is_file());

    let discovered = result(service.discover_packages())?;
    ensure!(
        discovered.iter().any(|found| found.package == "demo"),
        "discover_packages must list the newly created package"
    );
    Ok(())
}

#[test]
fn delete_contract_removes_file_and_manifest_entry_with_cas() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let service = templiqx_local::compose(temp.path())?;
    result(service.create_package(&CreatePackageRequest {
        name: "demo".into(),
        version: "0.1.0".into(),
    }))?;

    let source = r#"api_version: templiqx/v1alpha1
id: hello
version: 1.0.0
description: test
inputs: {}
context: {}
messages:
  - role: user
    content:
      - kind: text
        value: hi
output_schema:
  type: object
"#;
    let put = result(service.put_contract("demo", "hello", source, None))?;
    let contract_path = temp.path().join("demo/contracts/hello.yaml");
    ensure!(contract_path.is_file());

    result(service.delete_contract(&DeleteContractRequest {
        package: "demo".into(),
        contract: "hello".into(),
        expected_fingerprint: put.fingerprint,
    }))?;
    ensure!(!contract_path.exists());
    let manifest = result(service.discover_packages())?
        .into_iter()
        .find(|m| m.package == "demo")
        .context("demo manifest")?;
    ensure!(!manifest.contracts.iter().any(|id| id == "hello"));

    let wrong_fp = service.delete_contract(&DeleteContractRequest {
        package: "demo".into(),
        contract: "hello".into(),
        expected_fingerprint: "deadbeef".into(),
    });
    ensure!(!wrong_fp.ok);
    Ok(())
}

#[tokio::test]
async fn create_package_cli_mcp_and_rust_envelopes_match() -> Result<()> {
    let rust_root = tempfile::tempdir()?;
    let cli_root = tempfile::tempdir()?;
    let mcp_root = tempfile::tempdir()?;

    let rust_service = templiqx_local::compose(rust_root.path())?;
    let mcp_service = templiqx_local::compose(mcp_root.path())?;
    let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
    let server_task = tokio::spawn(async move {
        let running = TempliqxMcp::new(mcp_service)
            .serve(server_transport)
            .await?;
        running.waiting().await?;
        anyhow::Ok(())
    });
    let client = ().serve(client_transport).await?;

    let rust_create = rust_service.create_package(&CreatePackageRequest {
        name: "demo".into(),
        version: "0.1.0".into(),
    });
    let cli_create = cli_envelope(cli_root.path(), &["create", "demo", "--version", "0.1.0"])?;
    let mcp_create = mcp_call(
        &client,
        "create_package",
        json!({"name": "demo", "version": "0.1.0"}),
    )
    .await?;
    assert_equal_envelopes(&rust_create, &cli_create, &mcp_create)?;
    ensure!(rust_create.ok);

    client.cancel().await?;
    server_task.await??;
    Ok(())
}
