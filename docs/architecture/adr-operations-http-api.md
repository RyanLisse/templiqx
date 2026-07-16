---
title: "ADR: Operations HTTP API boundary"
---

## Status
Accepted (2026-07-14).

## Context
Templiqx already exposes actor-neutral operations through `TempliqxService` and stable `OperationEnvelope<T>` responses. Rust, CLI, and MCP callers should not fork Templiqx semantics. Opcos also need a language-neutral integration point for TypeScript, .NET, Python, and later other consumers without moving validation, diagnostics, fingerprints, path confinement, CAS checks, or runtime policy into generated SDKs.

The existing `templiqx-mock-gateway` is a conformance fixture gateway. It proves deterministic scenario behavior against mock manifests. It is not a production northbound API, auth layer, retrieval layer, model gateway, or SDK surface.

## Decision
Add a versioned northbound Operations API contract at `/operations/v1/*`, described by `openapi/templiqx-operations-v1.yaml`.

The API is a thin HTTP transport over `TempliqxService`:

- every operation returns the same `OperationEnvelope` shape used by Rust, CLI, and MCP;
- request bodies map to existing application-owned request DTOs where the service already has them;
- request IDs and tenant IDs are transport metadata only;
- host systems own authentication, authorization, tenant policy, provider secrets, model routing, retries, and rate limiting;
- Templiqx owns contract parsing, validation, compilation, runtime capability checks, diagnostics, fingerprints, path confinement, and CAS enforcement.

## Northbound vs southbound

| Direction | Owner | Purpose | Examples |
| --- | --- | --- | --- |
| Northbound | Templiqx transport surface | Stable HTTP operation contract for hosts and generated SDKs | `/operations/v1/catalog`, package discovery, contract inspect/validate/compile/execute, CAS mutations |
| Southbound | Host composition and Templiqx ports | Runtime, storage, document, import, and workspace adapters called by `TempliqxService` | `RuntimeAdapter`, `PackageStore`, `ArtifactWorkspace`, document renderer, legacy import adapter |

Northbound handlers may compose southbound ports, but SDKs never call southbound adapters directly. Generated SDKs remain transport clients, not language-specific Templiqx implementations.

## Mock gateway boundary

`templiqx-mock-gateway` remains conformance-only:

- it must not be imported or reused as the production Operations API transport;
- it may keep exposing mock scenario endpoints used by black-box conformance tests;
- it may compare HTTP outcomes with inventory expectations;
- it must not become the auth, tenant, retrieval, provider, or SDK contract.

## Versioning and compatibility

- The HTTP base path carries the major API version: `/operations/v1`.
- The response envelope `api_version` remains the product contract version, currently `templiqx/v1alpha1`.
- Compatible additions may add optional response fields or new operations under `/operations/v1`.
- Breaking wire changes require a new base path, for example `/operations/v2`.
- OpenAPI is the public source for generated SDKs. Checked-in generated clients are out of scope until real pilot usage proves the contract.

## Idempotency and retries

Templiqx transport does not retry operations internally. Hosts and SDKs may retry only operations documented as safe or idempotent by the OpenAPI operation metadata. CAS mutations require an `If-Match` fingerprint and must not be retried blindly after ambiguous transport failure.

## Non-goals

- Replacing CLI or MCP.
- Promoting `templiqx-mock-gateway` as production API.
- Implementing auth, tenant authorization, provider secrets, model routing, or retrieval inside Templiqx.
- Handwriting language SDK business logic.
- WASM-first transport or WASM distribution.

## Consequences

- TypeScript, .NET, Python, Go, and Rust pilot SDKs generate from one OpenAPI contract.
- Runtime/storage/document behavior remains application-owned and conformance-tested.
- HTTP parity can be tested black-box without giving the mock gateway production responsibilities.
- The runnable `templiqx-http-server` binary is a **local/demo** composition
  (`TEMPLIQX_RUNTIME_MODE=deterministic-fake` by default, optional `langfuse`).
  It is **not** an official signed release artifact; see
  [HTTP server release artifact](../adr/http-server-release-artifact.md).
- Discovery routes on `templiqx_http::router` include the checked-in OpenAPI
  document (`/operations/v1/openapi.json` / `.yaml`) and interactive Swagger UI
  at `/swagger-ui` (utoipa-swagger-ui pointed at that JSON). Operator narrative:
  [Operations HTTP API](../guides/operations-api.md).
