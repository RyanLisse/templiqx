# Generated client policy

Templiqx publishes one OpenAPI 3.1 contract
([`openapi/templiqx-operations-v1.yaml`](../../openapi/templiqx-operations-v1.yaml)).
Language SDKs are generated from that document; they must not re-implement
Templiqx validation, diagnostics, fingerprints, CAS, or runtime policy.

## First-wave languages

| Language | Status in repo | Notes |
| --- | --- | --- |
| TypeScript | Pilot SDK in `sdk/typescript/` | Generated DTOs with a hand-written transport-only façade and live conformance test. |
| .NET | Planned pilot | Generate after two real opco pilots validate the wire contract. |
| Python | Pilot SDK in `sdk/python/` | Generated Pydantic v2 DTOs with a hand-written synchronous `httpx` façade. |
| Go / Rust | Later | Add when a concrete consumer appears. |
| C++ | Consumer-driven only | No speculative generator work. |

## Repository policy

1. **Single source of truth** — OpenAPI under `openapi/` is normative for HTTP
   wire shape. Router handlers and integration tests must not drift from it.
2. **Deterministic checked-in DTOs** — Pilot SDKs check in generated model files
   only when their generator and drift check are pinned. Client façades remain
   small, hand-written transport adapters.
3. **Transport-only SDKs** — Generated methods map 1:1 to HTTP operations and
   deserialize `OperationEnvelope` responses. Business logic stays in
   `TempliqxService` on the server.
4. **Version pinning** — Host SDK releases track `/operations/v1` until a new
   base path is published. Product contract evolution remains visible through
   envelope `api_version`.

## Commands

```bash
npm run openapi:validate
npm run openapi:typescript-proof
just openapi-validate
just openapi-typescript-proof
```

Validation checks OpenAPI 3.1 structure, internal refs, versioned paths, required
operation ids, idempotency metadata on mutations, and core envelope schemas. The
TypeScript proof uses pinned `openapi-typescript` and `typescript` versions via
`npx` so local runs stay reproducible without adding generated files to git.

## Publishing gate

Do not publish package-registry SDKs from this repository until:

1. Two opco pilots have exercised the same `/operations/v1` contract against
   host-owned transports.
2. CI runs `openapi:validate` and `openapi:typescript-proof` on every change to
   the spec or HTTP router.
3. Breaking HTTP changes ship only through a new `/operations/vN` base path.
