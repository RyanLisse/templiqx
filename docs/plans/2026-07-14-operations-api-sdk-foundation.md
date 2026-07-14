# Templiqx Operations API and SDK foundation

## Goal

Add a versioned northbound HTTP API as a thin transport over `TempliqxService`, publish an OpenAPI 3.1 contract, and establish generated-client conformance without moving Templiqx semantics into SDKs. Keep the existing mock gateway conformance-only. WASM is out of scope.

## Ideal-state criteria

- A production-shaped HTTP transport exposes the canonical operation catalog and `OperationEnvelope` semantics.
- The OpenAPI document is versioned, validated, and served by the transport.
- Diagnostics, fingerprints, capability checks, path confinement, and CAS remain application-owned.
- Request/tenant correlation headers are transport metadata; authentication and authorization remain host policy.
- Retries are not performed by the service transport; the spec identifies safe/idempotent operations for clients.
- One black-box conformance suite proves raw HTTP parity with the application service.
- Generated SDKs can consume the same spec; no handwritten language implementation is required in this slice.

## Implementation

1. **Architecture contract**
   - Add an ADR separating northbound Operations API from southbound runtime/storage/document ports.
   - State that `templiqx-mock-gateway` must never be reused or imported by the production transport.
2. **HTTP transport crate**
   - Add `crates/templiqx-http` using Axum and `templiqx-local` composition.
   - Start with transport discovery plus representative read, execute, and CAS mutation routes, all delegating to `TempliqxService`.
   - Use `/operations/v1/*`, JSON request bodies, structured `OperationEnvelope` responses, health endpoints, request IDs, body limits, timeouts, and graceful shutdown.
3. **OpenAPI 3.1**
   - Add `openapi/templiqx-operations-v1.yaml` as the public wire contract.
   - Model operation envelopes, diagnostics, fingerprints, correlation headers, error status mapping, idempotency metadata, and representative routes.
   - Serve the exact checked-in document from `/operations/v1/openapi.json` or YAML.
4. **Conformance**
   - Add integration tests that run the HTTP router in-process and compare envelopes/fingerprints with direct service calls.
   - Assert unknown fields, invalid JSON, payload limits, request IDs, and CAS conflicts fail deterministically.
   - Add a guard proving the production HTTP crate does not depend on mock crates or the mock gateway.
5. **SDK foundation**
   - Add deterministic OpenAPI validation/generation commands and generated-client policy docs.
   - Prove one generated TypeScript client compiles against the spec, without committing broad generated output unless reproducible.
   - Record TypeScript, .NET, and Python as the usage-driven first wave; Go/Rust later and C++ only for a real consumer.
6. **Documentation and verification**
   - Document raw HTTP and SDK usage, versioning, compatibility matrix fields, host responsibilities, and non-goals.
   - Run fmt, clippy, workspace tests, boundary checks, OpenAPI validation, docs build, and HTTP conformance locally.

## Non-goals

- Replacing MCP or CLI.
- Promoting `templiqx-mock-gateway` to a production API.
- Implementing auth, tenant authorization, provider secrets, or model routing inside Templiqx.
- Six handwritten SDKs.
- A WASM transport or WASM-first distribution.

## Delivery slices

1. ADR + complete OpenAPI contract skeleton + HTTP router for catalog/discovery/inspect/validate/compile/execute.
2. Remaining lifecycle, eval, document, workspace, and trust operations.
3. Black-box conformance and generated TypeScript proof.
4. Publish SDK packages only after two real opco pilots validate the wire contract.
