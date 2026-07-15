---
title: Operations HTTP API
---

The northbound Operations API is a thin HTTP transport over the same
`TempliqxService` catalog used by Rust, CLI, and MCP. Every **operation route**
returns an `OperationEnvelope`; health (`/healthz`, `/operations/v1/health/*`)
and OpenAPI discovery routes return their own lightweight shapes. Transport adds
request IDs, body limits, and timeouts only.

See also: [ADR: Operations HTTP API boundary](../architecture/adr-operations-http-api.md)
and the checked-in contract at [`openapi/templiqx-operations-v1.yaml`](../../openapi/templiqx-operations-v1.yaml).

## Base path and versioning

| Layer | Value | Notes |
| --- | --- | --- |
| HTTP major version | `/operations/v1/*` | Breaking wire changes require a new base path (for example `/operations/v2`). |
| Product contract version | `templiqx/v1alpha1` | Returned as `api_version` on every operation envelope. |
| OpenAPI document version | `1.0.0-alpha.1` | Describes the HTTP wire contract, not the YAML contract grammar. |

Compatible additions may add optional response fields or new operations under
`/operations/v1`. SDKs and hosts should ignore unknown JSON fields on read.

## Discovery and health

| Route | Purpose |
| --- | --- |
| `GET /healthz`, `GET /readyz` | Legacy health aliases |
| `GET /operations/v1/health/live` | Liveness |
| `GET /operations/v1/health/ready` | Readiness |
| `GET /operations/v1/openapi.yaml` | Checked-in OpenAPI 3.1 (YAML) |
| `GET /operations/v1/openapi.json` | Same document (JSON) |
| `GET /operations/v1/catalog` | Canonical 27-operation catalog |

## Raw HTTP usage

Local composition (filesystem-backed service):

```bash
cargo run -p templiqx-cli -- --root examples/packages catalog
# equivalent HTTP shape once a host binds the router:
curl -sS http://localhost:8080/operations/v1/catalog
curl -sS http://localhost:8080/operations/v1/packages
curl -sS -X POST http://localhost:8080/operations/v1/packages/demo/contracts/greeting/compile \
  -H 'content-type: application/json' \
  -d '{"render":{"inputs":{"name":"Ryan"},"context":{"organization":"Blinqx"}},"capabilities":["structured_output"]}'
```

Hosts compose production adapters and bind the router themselves:

```rust
use std::net::SocketAddr;
use templiqx_http::{router, serve};

let service = /* host-owned TempliqxService composition */;
serve(router(service), "0.0.0.0:8080".parse().unwrap()).await?;
```

`serve` and `serve_from_root` drain in-flight requests on SIGINT/CTRL+C or
SIGTERM (Unix) before exit. Production hosts may wrap the same router in their
own process manager,
load balancer, and TLS termination.

## Transport metadata

| Header | Direction | Semantics |
| --- | --- | --- |
| `X-Request-Id` | Request (optional) / response (always) | Correlation id echoed when supplied; otherwise generated as `tqx-<n>`. |
| `X-Tenant-Id` | Request (optional) | Documented in OpenAPI for host policy; not interpreted by Templiqx core. |
| `If-Match` | Request on CAS mutations | Required for delete/update/sign paths; optional on contract `PUT`. |

JSON request bodies on strict DTOs reject unknown fields. Invalid JSON returns
`TQX_HTTP_JSON` diagnostics with HTTP 400. Bodies larger than 1 MiB return HTTP
413. Handler timeouts return HTTP 400 with `TQX_HTTP_TRANSPORT`.

## Compatibility matrix (v1)

| Concern | Transport | Application / SDK |
| --- | --- | --- |
| Operation semantics | Delegates unchanged to `TempliqxService` | Generated clients are thin HTTP wrappers only |
| Diagnostics and fingerprints | Pass-through in envelopes | Must not be re-modeled per language |
| CAS mutations | Requires `If-Match` | Clients must surface conflict diagnostics; no blind retries |
| Idempotency | Declared per operation via `x-templiqx-idempotent` in OpenAPI | Hosts/SDKs may retry only safe/idempotent operations |
| Auth / tenant / provider routing | Not implemented | Host responsibility |
| Retries | Not performed by transport | Host/SDK policy |

## Host responsibilities

- Bind TLS, authentication, authorization, tenant routing, and rate limiting in
  front of the router.
- Inject a host-composed `TempliqxService` through `router(service)`; do not
  import `templiqx-mock-gateway` into production transport.
- Publish the served OpenAPI document from `/operations/v1/openapi.json` or
  `.yaml` so SDK generation tracks the running transport.

## Non-goals

- Replacing CLI or MCP.
- Promoting `templiqx-mock-gateway` to a production API.
- Handwriting six language SDKs in this repository.
- WASM transport or WASM-first distribution.

## Verification

```bash
cargo test -p templiqx-http
npm run openapi:validate
npm run openapi:typescript-proof
./scripts/check-boundaries.sh
```
