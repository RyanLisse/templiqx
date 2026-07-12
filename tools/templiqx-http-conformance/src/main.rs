use std::{env, time::Duration};
use templiqx_contracts::{CompiledInteraction, CompiledMessage, ExecutionRequest, Role};
use templiqx_ports::RuntimeAdapter;
use templiqx_runtime_http_mock::HttpMockRuntime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url =
        env::var("TEMPLIQX_RUNTIME_URL").unwrap_or_else(|_| "http://mock-gateway:8080".into());
    let scenario =
        env::var("TEMPLIQX_RUNTIME_SCENARIO").unwrap_or_else(|_| "intake-document-01".into());
    let runtime = HttpMockRuntime::new(&url, scenario, Duration::from_secs(5))?;
    let receipt = runtime.execute(&ExecutionRequest {
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
    })?;
    if !receipt.output.is_null() {
        return Err("HTTP mock receipt carried a payload".into());
    }
    println!(
        "{}",
        serde_json::json!({"api_version":"templiqx/http-conformance/v1", "request_fingerprint":receipt.request_fingerprint, "output_fingerprint":receipt.output_fingerprint, "schema_valid":receipt.output_schema_valid})
    );
    Ok(())
}
