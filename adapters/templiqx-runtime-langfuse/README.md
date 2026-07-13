# Templiqx Langfuse-traced runtime adapter

`templiqx-runtime-langfuse` is a host-owned `RuntimeAdapter` implementation.
It calls a real OpenAI-compatible `POST {base_url}/chat/completions` endpoint
and, best-effort, ships a trace + generation event to Langfuse's ingestion
API (`POST {langfuse_host}/api/public/ingestion`).

This is production adapter code, not a conformance mock — construct it in
host code that owns the model/Langfuse credentials, and inject it wherever
your host composes a `RuntimeAdapter`. It is intentionally not wired into
`templiqx-application`, `templiqx-cli`, or `templiqx-mcp`'s default
composition; those surfaces stay adapter-agnostic by design (see
`docs/architecture/poc.md`).

## Behavior

- One chat completion request per `execute()` call. No retries — hosts own
  retry/backoff policy, same convention as `templiqx-runtime-http-mock`.
- The request uses the OpenAI-compatible `response_format.json_schema` shape,
  with the compiled interaction's schema and strict structured output enabled.
- The model's response content is bounded to 2 MiB, parsed as JSON, and
  validated against the same `output_schema`; `output_schema_valid` reflects
  that check.
- Langfuse tracing is fire-and-forget: a Langfuse outage is logged to stderr
  and never fails contract execution.
- Provider response bodies are excluded from runtime diagnostics. Configured
  credentials are redacted before a diagnostic fingerprint is produced.

## Failure mapping

- HTTP `429`: `TQX_RUNTIME_RATE_LIMITED`
- HTTP `5xx`: `TQX_RUNTIME_UNAVAILABLE`
- other non-2xx HTTP: `TQX_RUNTIME_PERMANENT`
- connection/DNS failures: `TQX_RUNTIME_UNAVAILABLE`
- transport timeout: `TQX_RUNTIME_TIMEOUT`
- malformed response body or non-JSON model output: `TQX_RUNTIME_INVALID_RESPONSE`

For `429`, numeric `x-retry-after-ms` takes precedence over numeric
`Retry-After` seconds and is normalized to `RuntimeFailure.retry_after_ms`.

## Known ceiling

Uses Langfuse's legacy batch ingestion endpoint. Langfuse's own docs now
point new integrations at the OTLP endpoint instead; swap `emit_trace` for
an OTLP exporter if/when that migration matters.

This adapter intentionally uses `RuntimeAdapter`'s provider-neutral streaming
fallback (one terminal `Complete` event). Live OpenAI-compatible SSE is not
implemented here: its wire format is provider-specific and should be supplied
by a host adapter that can prove ordering, cancellation, tool-call fragments,
and terminal error behavior against its chosen provider.
