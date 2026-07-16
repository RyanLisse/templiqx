---
title: "ADR: Operations HTTP server is not a signed release artifact"
---

## Status
Accepted (2026-07-16).

## Context
The repository ships `crates/templiqx-http-server` and a Dockerfile target
`templiqx-http-server` used by Compose and the Helm chart for local/demo
Operations API smoke. Tag release workflow
(`.github/workflows/release.yml`) publishes and Cosign-signs only three OCI
images: `templiqx-cli`, `templiqx-mcp`, and synthetic `templiqx-conformance`.

Operators and docs previously mixed “product HTTP server” wording with
deterministic-fake defaults, which looked like a fourth signed release.

## Decision
**No.** `templiqx-http-server` is **not** an official signed release artifact.

| Surface | Release status |
| --- | --- |
| `templiqx-cli` | Signed OCI release |
| `templiqx-mcp` | Signed OCI release |
| `templiqx-conformance` | Signed OCI release (synthetic-only) |
| Helm chart (`templiqx-*.tgz`) | Signed blob release (conformance-pinned) |
| `templiqx-http-server` | **Local/demo Docker target only** — unsigned, not published by `release.yml` |
| `templiqx-http` library (`router`) | Host-composed production transport seam |

Rationale:

1. Production hosts own auth, TLS, tenant policy, model routing, and process
   lifecycle. They should bind `templiqx_http::router` (or an equivalent host
   process) rather than consume an unsigned demo binary as a platform service.
2. The binary defaults to `TEMPLIQX_RUNTIME_MODE=deterministic-fake`, which is
   explicitly non-production.
3. Signing a fourth image without host hardening would over-claim readiness.

## Consequences

- Release docs, OCI labels, Compose, and Helm must state the HTTP server is
  local/demo and unsigned.
- `scripts/release-validate.sh` and `release.yml` continue to require exactly
  three image targets.
- Revisit only if a concrete host asks for a signed Operations HTTP image and
  accepts explicit mode gates (`langfuse` or host-injected runtime) plus auth
  boundary ownership documentation.

## See also

- [Releasing](../guides/releasing.md)
- [Operations HTTP API boundary](../architecture/adr-operations-http-api.md)
- [Host integration](../guides/host-integration.md)
