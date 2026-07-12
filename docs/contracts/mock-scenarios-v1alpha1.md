# Mock scenario format `templiqx.mock/v1alpha1`

`templiqx.mock/v1alpha1` is a conformance-only scenario DTO parsed by
`templiqx-mock`. It is not a production workflow, auth, retrieval or model
provider API. Hosts own those concerns around Templiqx.

Every scenario is strict serde data with `deny_unknown_fields`.

The checked-in CRM3 corpus is enumerated by
`examples/crm3/scenarios/inventory.json`; the inventory is the source of truth
for scenario discovery rather than directory scanning.

```json
{
  "api_version": "templiqx.mock/v1alpha1",
  "id": "intake-document-01",
  "contract": "bli-61-date-term-extraction",
  "kind": "happy_path",
  "input": "../../evals/bli-61-request.json",
  "expected_output": "../../evals/bli-61-output.json",
  "expected_output_fingerprint": "66d6c3fbb03611b4b22deabbe9ef669e5ad69f59c1e90be4dee92d9c3a5188de",
  "expected_diagnostics": [],
  "receipt_payload_policy": "fingerprints_only",
  "steps": [
    { "id": "request-received", "kind": "request_received" },
    { "id": "runtime-latency", "kind": "delay", "delay_ms": 25 },
    { "id": "runtime-success", "kind": "runtime_success", "output_schema_valid": true }
  ]
}
```

Scenarios may use `events` instead of `steps` for a typed stream:

```json
{"events":[
  {"kind":"start","id":"s"},
  {"kind":"reasoning","id":"r","text":"ground facts"},
  {"kind":"delta","id":"d","text":"result"},
  {"kind":"usage","id":"u","input_tokens":2,"output_tokens":1},
  {"kind":"end","id":"e"},
  {"kind":"finish","id":"f","output_schema_valid":true}
]}
```

## Fields

- `api_version`: must be exactly `templiqx.mock/v1alpha1`.
- `id`: stable scenario id, unique in the CRM3 scenario inventory.
- `contract`: contract id executed by the conformance harness.
- `kind`: one of `happy_path`, `ambiguous`, `missing`, `invalid`, `drafting`,
  `failure`, `document_warning`.
- `input`: optional path relative to the scenario manifest.
- `expected_output`: optional path relative to the scenario manifest.
- `expected_output_fingerprint`: SHA-256 over Templiqx canonical JSON output.
- `expected_diagnostics`: stable diagnostic codes expected from execution.
- `expected_status` and `expected_failure`: exact success/failure expectation.
- `receipt_payload_policy`: `fingerprints_only` or `no_successful_receipt`.
- `steps`: ordered runtime lifecycle events.
- `events`: mutually exclusive typed aggregate stream events.
- `evidence` and `document_expectation`: payload-free conformance expectations.
- `golden_receipt_fingerprint`: exact payload-free receipt/conformance fingerprint.

## Lifecycle

The parser rejects:

- empty `steps`;
- duplicate step ids;
- unsupported API versions or unknown fields;
- `delay` before `request_received`;
- `delay` with missing or zero `delay_ms`;
- terminal events before `request_received`;
- any step after `runtime_success` or `runtime_failure`;
- `runtime_failure` without a `failure` object.

Typed streams must contain a unique `start`, optional non-empty `delta`,
`reasoning`, and `usage` events, followed by `end` and `finish`. `finish`
before `start`, duplicate event ids, missing lifecycle terminals, and unknown
event fields are rejected. The synchronous adapter concatenates deltas and
reasoning in event order and sums usage deterministically; an explicit finish
output takes precedence when supplied.

These fixture events are the scenario DTO `ScenarioStreamEvent` (Rust name;
JSON tags unchanged) and describe a model's aggregate stream lifecycle. They
are distinct from the runtime `templiqx_contracts::StreamEvent` emitted by
`RuntimeAdapter::execute_streaming` (`Delta`, `ToolCallDelta`, `Complete`,
`Failed`). When executed with streaming, `ScriptedRuntime` replays each fixture
`delta` as a `StreamEvent::Delta`, then emits a terminal `Complete` carrying the
exact receipt `execute` produces (fingerprint parity) — or a `Failed` event with
a stable `TQX_*` code before the error returns.

Delays advance an injected virtual clock only. The mock runtime never sleeps.

## Fingerprint Policy

Scenario fingerprints are payload-free. They include scenario identity,
diagnostics, receipt policy, expected output fingerprints and lifecycle steps.
They do not include CRM3 request text or model output payloads.

## Conformance HTTP gateway

`tools/templiqx-mock-gateway` is the only HTTP transport for these fixtures. It
is not a production Templiqx API or an MCP transport. Start it with
`--listen` and `--scenario-root`; `TEMPLIQX_MOCK_SCENARIO_ROOT` supplies the
scenario root when the flag is omitted. It exposes `GET /health/live`,
`GET /health/ready`, `GET /v1/scenarios`, and
`POST /v1/scenarios/{id}/execute`. Execution returns typed success or failure
data, virtual elapsed time, attempts, and fingerprints without returning output
payloads.
