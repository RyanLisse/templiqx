# ADR: Streaming `RuntimeAdapter` port extension

## Status

Implemented (2026-07-12) — the `execute_streaming` port method, the
`StreamEvent` contracts type, deterministic mock replay, the
`execute_contract(stream)` application flag, and the CLI `--stream` / MCP
`stream` surfaces have all landed, covered by
`crates/templiqx-conformance/tests/streaming.rs`. Superseded the prior
"design only" status. A live streaming proof against a real host adapter is
still outstanding (R12: "Partially — live streaming proof needs host").

## Context

`RuntimeAdapter` (`templiqx-ports`) is a single synchronous, non-streaming
method:

```rust
pub trait RuntimeAdapter: Send + Sync {
    fn descriptor(&self) -> AdapterDescriptor;
    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError>;
}
```

`templiqx-mock` and `templiqx-runtime-langfuse` both implement `execute` as a
single request/response round trip: build the interaction, call the model
once, validate and fingerprint the output, return one `ExecutionReceipt`.
Real chat-completion and agent-runtime APIs commonly stream partial output
(token deltas, tool-call fragments) before the final response. R9 already
names streaming as adapter-scope, host-owned, outside the deterministic
fake/conformance adapter. This ADR specifies how a streaming adapter plugs
into the same port without breaking `execute` or the deterministic mock.

## Decision

1. **Additive trait method with a default, not a breaking signature change.**
   `RuntimeAdapter` gains a second method with a default implementation that
   falls back to `execute`, so every existing implementor (`templiqx-mock`,
   `templiqx-runtime-langfuse`, any host adapter) keeps compiling unchanged:

   ```rust
   pub trait RuntimeAdapter: Send + Sync {
       fn descriptor(&self) -> AdapterDescriptor;
       fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError>;

       /// Optional streaming path. Default forwards to `execute` and emits
       /// the whole receipt as a single terminal event — adapters that don't
       /// stream need no code change.
       fn execute_streaming(
           &self,
           request: &ExecutionRequest,
           sink: &mut dyn FnMut(StreamEvent),
       ) -> Result<ExecutionReceipt, PortError> {
           let receipt = self.execute(request)?;
           sink(StreamEvent::Complete(receipt.clone()));
           Ok(receipt)
       }
   }
   ```

2. **`StreamEvent` is a new, minimal contracts type — not a new receipt shape.**

   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
   #[serde(deny_unknown_fields, tag = "kind")]
   pub enum StreamEvent {
       Delta { text: String },
       ToolCallDelta { name: String, arguments_fragment: String },
       Complete(ExecutionReceipt),
       Failed { code: String, message: String },
   }
   ```

   The final `Complete` event always carries the same `ExecutionReceipt` that
   a non-streaming `execute` call would have produced — fingerprints,
   `output_schema_valid`, and `output` are computed identically regardless of
   whether the caller streamed. Streaming is a transport/observability
   concern layered over the same deterministic result contract, not a second
   output format.

3. **Callback sink, not an async stream type.** The port crate has no async
   runtime dependency today (`execute` is synchronous). A `FnMut(StreamEvent)`
   callback keeps the trait sync and dependency-free; a host adapter that
   wraps a real async streaming API (SSE, websockets) drives the callback
   from its own executor and blocks `execute_streaming` until the terminal
   event, exactly as `templiqx-runtime-langfuse::execute` already blocks on
   its HTTP call.

4. **Mock replays fixture deltas deterministically (implemented deviation).**
   The design originally left `templiqx-mock` on the default method. As built,
   `ScriptedRuntime` *does* override `execute_streaming`: for a scenario with an
   `events` fixture it emits each fixture `delta` as a contracts
   `StreamEvent::Delta`, then calls `execute` and emits the terminal event —
   `Complete(receipt)` on success or `Failed { code, message }` on error. The
   terminal `Complete` still carries the exact receipt `execute` produces, so
   fingerprint parity holds and CRM3 golden receipts stay untouched. The mock's
   own fixture-lifecycle enum was renamed `StreamEvent` → `ScenarioStreamEvent`
   to free the canonical name for the contracts type; scenario JSON tags are
   unchanged.

## Consequences

- Adding a real streaming adapter (host-owned, e.g. an OpenAI-compatible SSE
  client) requires zero changes to `templiqx-contracts`'s `ExecutionReceipt`,
  `templiqx-application`, the CLI, or the MCP surface — only a new
  `execute_streaming` override in that adapter crate.
- `templiqx-cli`/`templiqx-mcp` can add an opt-in `--stream` flag later that
  calls `execute_streaming` with a sink that writes `Delta` events to
  stdout/stderr as they arrive, falling back to today's behavior when the
  active adapter doesn't override the default.
- No change to `PortError`, `AdapterDescriptor`, or boundary rules — streaming
  stays entirely within the existing `templiqx-ports` / adapter split.

## Alternatives considered

- **Separate `StreamingRuntimeAdapter` trait.** Rejected — forces every
  caller (CLI, MCP, application service) to special-case two adapter kinds.
  A default-method extension keeps one trait, one capability surface.
- **`async fn` / `Stream` return type.** Rejected for this slice — would pull
  an async runtime into `templiqx-ports`, which today has none, and none of
  the current synchronous callers (CLI, conformance harness) need it.
- **Encode partial deltas into `ExecutionReceipt` itself.** Rejected — would
  make the receipt shape depend on whether streaming was used, breaking the
  "same outcome for humans and agents, same artifact" invariant (R11,
  human-agent outcome parity).

## Resolved questions

- **`StreamEvent::Failed` vs `PortError`:** a mid-stream failure does both — it
  emits one `StreamEvent::Failed { code, message }` (a stable `TQX_*` code, so
  streaming observers see a typed terminal event) *and* returns the underlying
  `PortError` from `execute_streaming`. The `Result` remains the authoritative
  outcome; the `Failed` event is the streaming projection of it.

## Open questions

- A live streaming proof against a real host adapter (e.g. token-by-token
  Langfuse) is still outstanding — the mock replays fixture deltas, but no
  production adapter overrides `execute_streaming` yet (R12).
