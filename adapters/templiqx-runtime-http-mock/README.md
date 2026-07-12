# Templiqx mock HTTP runtime adapter

`templiqx-runtime-http-mock` is a conformance-only `RuntimeAdapter`. It sends
one JSON `ExecutionRequest` to `POST /v1/scenarios/{id}/execute` on the mock
gateway and maps its typed, payload-free outcome to the host `ExecutionReceipt`
port. Successful receipts contain no output payload (`null`); fingerprints and
schema validity are authoritative.

The adapter never retries. Hosts own retry and backoff policy.

## Failure mapping

- connection or DNS failures, and HTTP `503`: `TQX_RUNTIME_UNAVAILABLE`
- transport read/write/connect timeout: `TQX_RUNTIME_TIMEOUT`
- malformed HTTP, non-success HTTP other than `503`, empty bodies, or malformed
  receipt JSON: `TQX_RUNTIME_INVALID_RESPONSE`

It does not add a production API or MCP role.
