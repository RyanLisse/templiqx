# Observability seam

## Status

Accepted (2026-07-12). The optional Langfuse adapter this doc describes merged
into `main` the same day (`adapters/templiqx-runtime-langfuse`, `6da11b2`);
live-trace verification against a real Langfuse project still needs a host, so
R10 implementation proof stays host-gated while the seam design is complete.
Deterministic loopback tests do prove local request mapping, trace-outage
isolation, response bounds, failure normalization, and credential-safe
diagnostics without making external calls.

## What Templiqx core emits today

Every operation returns an `OperationEnvelope<T>` (`templiqx-contracts`):

```rust
pub struct OperationEnvelope<T> {
    pub api_version: String,
    pub operation: String,
    pub ok: bool,
    pub result: Option<T>,
    pub diagnostics: Vec<Diagnostic>,
    pub fingerprints: BTreeMap<String, String>,
}
```

`Diagnostic { code, severity, message, file, json_pointer, span, help }` is
already structured, already the single channel every diagnostic (parse error,
validation failure, boundary violation) flows through, and already
serializes deterministically. This is the export contract — there is no
second, adapter-specific diagnostic format to design. R10 is about *where
these get sent*, not changing their shape.

## The seam: host-owned, not core-owned

Nothing in `templiqx-contracts`/`templiqx-core`/`templiqx-application` calls
out to an external trace collector. Exporting diagnostics and execution
receipts to an observability backend (Langfuse, otherwise) is entirely a
host-adapter concern, same as `RuntimeAdapter` execution itself:

- `templiqx-runtime-langfuse` (host-owned, `adapters/`, not in default
  composition) implements `RuntimeAdapter::execute`. After each real model
  call it best-effort posts a trace (prompt, completion, usage) to Langfuse's
  ingestion API. A failed trace post never fails the underlying
  `ExecutionReceipt` — tracing is observability, not a correctness gate.
- The CLI/MCP/application layer never imports Langfuse or any tracing SDK.
  `scripts/check-boundaries.sh` enforces this: default composition
  (`templiqx-application`, `templiqx-cli`, `templiqx-mcp`) may not depend on
  host-owned adapters.

This mirrors the existing deployment boundary
(`docs/architecture/deployment.md`): core stays provider-neutral; a host
wires in whichever adapter it needs, with its own credentials, entirely
outside the package graph the conformance suite exercises.

## What a host wires up

```rust
// host-owned composition root, not in this repo's default binaries
let adapter = LangfuseTracedRuntime::new(
    ModelConfig {
        base_url: env("MODEL_GATEWAY_URL"),
        api_key: env("MODEL_GATEWAY_API_KEY"),
        model: env("MODEL_ID"),
        timeout,
    },
    LangfuseConfig {
        host: env("LANGFUSE_BASE_URL"),
        public_key: env("LANGFUSE_PUBLIC_KEY"),
        secret_key: env("LANGFUSE_SECRET_KEY"),
    },
)?;
let service = TempliqxService::new(storage, adapter, /* ... */);
```

`descriptor()` reports adapter identity (name/version) into the same
`AdapterDescriptor` every `ExecutionReceipt` already carries, so a trace and
its receipt are correlatable by `request_fingerprint` without any new
correlation-ID plumbing.

## Consequences

- No new export format to version — `Diagnostic`/`OperationEnvelope`/
  `ExecutionReceipt` are already the contract; a tracing adapter reads them,
  it doesn't extend them.
- Adding a second backend (e.g. an OpenTelemetry adapter) later is symmetric
  with adding Langfuse: a new `adapters/templiqx-runtime-otel` crate
  implementing `RuntimeAdapter`, boundary-checked, never touching core.
- Streaming traces (per-delta spans) become possible once
  `adr-streaming-runtime-port.md`'s `execute_streaming` lands — a streaming
  adapter can emit incremental Langfuse spans per `StreamEvent::Delta`
  instead of one span per `Complete`. No core change required for that
  either.
- Model HTTP responses are capped at 2 MiB. Provider response bodies never
  enter `RuntimeFailure.detail`; configured credentials are redacted before
  diagnostics and their fingerprints are created. Numeric retry hints are
  normalized into `retry_after_ms`.

## Alternatives considered

- **Built-in tracing in `templiqx-application`.** Rejected — would put a
  specific backend's SDK, credentials, and network calls into the portable
  core, violating the same boundary rule that keeps model-provider SDKs out
  of `templiqx-core`.
- **New `Trace`/`Span` types in `templiqx-contracts`.** Rejected — the
  existing `Diagnostic`/`ExecutionReceipt`/`AdapterDescriptor` triple already
  carries everything a trace needs (what happened, what it cost in terms of
  schema validity, which adapter ran it); a parallel tracing DTO would just
  be a second encoding of the same facts.
