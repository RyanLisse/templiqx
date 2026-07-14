//! Streaming `RuntimeAdapter` conformance (U1 / ADR streaming-runtime-port).
//!
//! Locks the three invariants of the streaming port:
//! (a) fingerprint parity — a streaming receipt is byte-for-byte identical to
//!     the non-streaming receipt for the same scenario (AE1);
//! (b) the trait's default `execute_streaming` emits exactly one `Complete`
//!     for adapters that do not override it;
//! (c) a mid-stream failure surfaces a `Failed` event with a stable diagnostic
//!     code and still returns the underlying error.

use anyhow::{Result, bail, ensure};
use std::path::{Path, PathBuf};
use templiqx_contracts::{
    AdapterDescriptor, CompiledInteraction, CompiledMessage, ExecutionReceipt, ExecutionRequest,
    Role, StreamEvent,
};
use templiqx_mock::{ScenarioManifest, ScriptedRuntime};
use templiqx_ports::{PortError, RuntimeAdapter, RuntimeFailureCode};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

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
            output_schema: serde_json::json!({"type": "object"}),
            required_capabilities: vec![],
            target_capabilities: vec![],
            runtime_policy: Default::default(),
            extensions: Default::default(),
        },
        fixture_output: Some(serde_json::json!({"ok": true})),
    }
}

fn streaming_manifest() -> ScenarioManifest {
    let json = serde_json::json!({
        "api_version": "templiqx.mock/v1alpha1",
        "id": "streaming-parity",
        "contract": "bli-61-date-term-extraction",
        "kind": "happy_path",
        "receipt_payload_policy": "fingerprints_only",
        "events": [
            {"kind": "start", "id": "s1"},
            {"kind": "delta", "id": "d1", "text": "Extracted "},
            {"kind": "delta", "id": "d2", "text": "onboarding facts"},
            {"kind": "end", "id": "e1"},
            {"kind": "finish", "id": "f1", "output": {"ok": true}, "output_schema_valid": true}
        ]
    });
    ScenarioManifest::from_json_slice(&serde_json::to_vec(&json).expect("serialize manifest"))
        .expect("valid streaming manifest")
}

#[test]
fn streaming_receipt_matches_non_streaming() -> Result<()> {
    let request = request();

    let plain = ScriptedRuntime::from_manifest(streaming_manifest());
    let non_streaming = plain.execute(&request)?;

    let scripted = ScriptedRuntime::from_manifest(streaming_manifest());
    let mut events = Vec::new();
    let streaming = scripted.execute_streaming(&request, &mut |event| events.push(event))?;

    ensure!(
        streaming.request_fingerprint == non_streaming.request_fingerprint
            && streaming.output_fingerprint == non_streaming.output_fingerprint,
        "streaming fingerprints diverge from non-streaming"
    );
    ensure!(
        streaming == non_streaming,
        "streaming receipt diverges from non-streaming"
    );

    let deltas = events
        .iter()
        .filter(|event| matches!(event, StreamEvent::Delta { .. }))
        .count();
    ensure!(
        deltas == 2,
        "expected 2 fixture deltas replayed, got {deltas}"
    );
    ensure!(
        matches!(events.last(), Some(StreamEvent::Complete(receipt)) if *receipt == streaming),
        "stream must terminate in Complete carrying the parity receipt"
    );
    Ok(())
}

struct PlainRuntime {
    receipt: ExecutionReceipt,
}

impl RuntimeAdapter for PlainRuntime {
    fn descriptor(&self) -> AdapterDescriptor {
        self.receipt.adapter.clone()
    }

    fn execute(&self, _request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError> {
        Ok(self.receipt.clone())
    }
}

#[test]
fn default_streaming_emits_single_complete() -> Result<()> {
    let receipt = ExecutionReceipt {
        adapter: AdapterDescriptor {
            id: "plain-runtime".into(),
            version: "0.0.0".into(),
            capabilities: vec![],
        },
        request_fingerprint: "req".into(),
        output_fingerprint: "out".into(),
        output: serde_json::json!({"ok": true}),
        output_schema_valid: true,
    };
    let runtime = PlainRuntime {
        receipt: receipt.clone(),
    };

    let mut events = Vec::new();
    let returned = runtime.execute_streaming(&request(), &mut |event| events.push(event))?;

    ensure!(
        returned == receipt,
        "default streaming must return the execute receipt"
    );
    ensure!(
        events.len() == 1,
        "default streaming must emit exactly one event, got {}",
        events.len()
    );
    ensure!(
        matches!(&events[0], StreamEvent::Complete(complete) if *complete == receipt),
        "the single event must be Complete carrying the receipt"
    );
    Ok(())
}

#[test]
fn mid_stream_failure_emits_stable_failed_code() -> Result<()> {
    let runtime = ScriptedRuntime::failure("mid-stream", RuntimeFailureCode::InvalidResponse);

    let mut events = Vec::new();
    let outcome = runtime.execute_streaming(&request(), &mut |event| events.push(event));

    ensure!(outcome.is_err(), "mid-stream failure must still return Err");
    ensure!(
        events.len() == 1,
        "failure stream must emit exactly one Failed event, got {}",
        events.len()
    );
    match &events[0] {
        StreamEvent::Failed { code, .. } => ensure!(
            code == "TQX_RUNTIME_INVALID_RESPONSE",
            "unexpected failure code: {code}"
        ),
        other => bail!("expected Failed event, got {other:?}"),
    }
    Ok(())
}

fn validate_event_order(events: &[StreamEvent]) -> Result<()> {
    ensure!(!events.is_empty(), "stream must contain a terminal event");
    let terminals: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(event, StreamEvent::Complete(_) | StreamEvent::Failed { .. }))
        .collect();
    ensure!(
        terminals.len() == 1,
        "stream must contain exactly one terminal event"
    );
    ensure!(
        terminals[0].0 == events.len() - 1,
        "terminal event must be last"
    );
    ensure!(
        events[..events.len() - 1].iter().all(|event| matches!(
            event,
            StreamEvent::Delta { .. } | StreamEvent::ToolCallDelta { .. }
        )),
        "only delta variants may precede the terminal event"
    );
    Ok(())
}

#[test]
fn stream_contract_covers_every_variant_and_rejects_invalid_ordering() -> Result<()> {
    let receipt = ExecutionReceipt {
        adapter: AdapterDescriptor {
            id: "provider".into(),
            version: "1".into(),
            capabilities: vec![],
        },
        request_fingerprint: "request".into(),
        output_fingerprint: "output".into(),
        output: serde_json::json!({"ok":true}),
        output_schema_valid: true,
    };
    let valid = vec![
        StreamEvent::Delta {
            text: "partial".into(),
        },
        StreamEvent::ToolCallDelta {
            name: "lookup".into(),
            arguments_fragment: "{}".into(),
        },
        StreamEvent::Complete(receipt.clone()),
    ];
    validate_event_order(&valid)?;
    validate_event_order(&[StreamEvent::Failed {
        code: "TQX_RUNTIME_TIMEOUT".into(),
        message: "timed out".into(),
    }])?;

    for invalid in [
        vec![
            StreamEvent::Complete(receipt.clone()),
            StreamEvent::Delta {
                text: "late".into(),
            },
        ],
        vec![
            StreamEvent::Complete(receipt.clone()),
            StreamEvent::Complete(receipt.clone()),
        ],
        vec![StreamEvent::Delta {
            text: "unterminated".into(),
        }],
    ] {
        ensure!(
            validate_event_order(&invalid).is_err(),
            "invalid stream ordering was accepted"
        );
    }
    Ok(())
}

/// U7: when stream=true, envelope carries stream_events; when false, field is empty.
#[test]
fn execute_contract_stream_events_only_when_streaming() -> Result<()> {
    let root = repo_root().join("examples");
    let service = templiqx_local::compose(&root)?;
    let capabilities = ["structured_output".to_string()];
    let request_path = root.join("crm3/evals/bli-61-request.json");
    let output_path = root.join("crm3/evals/bli-61-output.json");
    let request: templiqx_contracts::RenderRequest =
        serde_json::from_slice(&std::fs::read(request_path)?)?;
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(output_path)?)?;

    let plain = service.execute_contract(
        "crm3",
        "bli-61-date-term-extraction",
        &request,
        &capabilities,
        Some(fixture.clone()),
        false,
    );
    ensure!(plain.ok);
    ensure!(plain.stream_events.is_empty());

    let streaming = service.execute_contract(
        "crm3",
        "bli-61-date-term-extraction",
        &request,
        &capabilities,
        Some(fixture),
        true,
    );
    ensure!(streaming.ok);
    ensure!(
        !streaming.stream_events.is_empty(),
        "stream=true must populate stream_events"
    );
    ensure!(
        plain.fingerprints.get("output") == streaming.fingerprints.get("output"),
        "streaming must preserve output fingerprint parity"
    );
    Ok(())
}
