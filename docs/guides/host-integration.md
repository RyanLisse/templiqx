---
title: Host integration guide
---

This guide extracts the ownership matrix and handoff procedures from the pre-CRM3
readiness specification so Basenet CRM3 and other opco hosts can integrate without
opening the HTML spec.

**Synthetic proof is not production validation.** Conformance fixtures under
`examples/crm3/` and `examples/packages/` contain no customer data and no live
ModelGateway wiring. Host teams must replace synthetic fixtures with sanitized
production data before claiming production readiness.

## Ownership matrix

| Capability | Templiqx repo | Mock / conformance | CRM3 / opco host |
|------------|---------------|--------------------|------------------|
| Contract parse / validate / compile | Owner | Fixtures | Consumer |
| Model execution | `RuntimeAdapter` port | Scripted + HTTP mock | ModelGateway adapter + anonymizer |
| Grounded evidence schema | Validation | Golden offsets / hashes | Retrieval and source authorization |
| Proposal / approval / audit | No ownership | Consumer-contract harness | Owner per ADR-0016 |
| Retry / workflow | Typed failures | Scripted attempts | ModelGateway / BullMQ per ADR-0006 |
| Document rendering | Port + V5 adapter | DOCX baseline | Storage, permissions, lifecycle |
| Document preflight (`inspect_document`) | Port + V5 adapter | Legacy corpus inspect fixtures | Host template storage only |
| PDF / conversion | ADR entry criteria + typed seam | Recorded fixture metadata only | Host-constructed converter adapter |
| Authorized merge context | Fingerprint + fail-closed binding | Synthetic `fixtures/authorized-context.json` | Host validator supplies scope, policy, provenance |
| Translation bundle policy | Package artifacts + filters | Demo `translations/` | Tenant locale selection |
| Kubernetes runtime | Separate CLI/MCP artifacts | Synthetic conformance image, 8 Jobs, mock gateway | Host chart or sidecar integration |

## ModelGateway consumer contract

The host implements `RuntimeAdapter` (or HTTP transport compatible with
`templiqx-runtime-http-mock`) and runs the same scenario suite as the mock gateway.

### Scenario inventory

Scenarios live under `examples/crm3/scenarios/` with inventory at
`examples/crm3/scenarios/inventory.json`. Each scenario has a manifest under
`examples/crm3/scenarios/<id>/manifest.json`.

The complete adapter-parity inventory is:

| Scenario | Purpose |
|----------|---------|
| `intake-document-01` | Happy-path extraction → draft → DOCX |
| `draft-with-citations` | Grounded drafting with evidence links |
| `invalid-output-schema` | Schema rejection before downstream steps |
| `ambiguous-date` | Typed runtime failure (no invented dates) |
| `contradictory-evidence` | Permanent failure on conflicting evidence |
| `missing-required-field` | Missing field handling |
| `missing-notice-date` | Invalid runtime response for an absent notice date |
| `docx-unresolved-reference` | DOCX merge-field edge case |

### HTTP conformance runner

Use `tools/templiqx-http-conformance` against the host adapter URL:

```sh
export TEMPLIQX_RUNTIME_URL=https://host-model-gateway.example/v1
export TEMPLIQX_RUNTIME_SCENARIO=intake-document-01
templiqx-http-conformance
```

Run every ID from the inventory, not a hand-maintained subset. The runner checks
the declared status, diagnostic, schema result, output fingerprint, and receipt
fingerprint. Compare the normalized aggregate receipt with
`scripts/golden/http-conformance.json`. The mock gateway
(`tools/templiqx-mock-gateway`) is the reference transport; it must not ship as
a production adapter.

### Rust / CLI / MCP parity

Run the full CRM3 conformance suite locally before pointing at the host:

```sh
cargo test -p templiqx-conformance --test crm3
just verify
```

CLI entrypoint:

```sh
templiqx --root examples crm3-conformance --workspace /tmp/templiqx-workspace
```

## Fixture replacement checklist

When CRM3 host integration is ready:

1. Replace synthetic Dutch legal text in scenario manifests with sanitized CRM3
   source fragments (legal review required).
2. Keep evidence offsets and grounding checks — draft output must remain tied
   to source fragments (no invented facts).
3. Run `cargo test -p templiqx-conformance --test crm3` against host adapter
   with the same scenario IDs.
4. Update golden receipts only with `GOLDEN_REVIEW:` in the commit message.
5. Select a real second Blinqx opco package (post-CRM3, R13) after synthetic
   portability is proven via `examples/packages/synthetic-opco/`.

## Deployment artifacts

| Artifact | Location | Host use |
|----------|----------|----------|
| CLI OCI image | `Dockerfile` target `templiqx-cli` | Sidecar or Job runner |
| MCP OCI image | `Dockerfile` target `templiqx-mcp` | Agent stdio transport |
| Synthetic conformance OCI image | `Dockerfile` target `templiqx-conformance` | Contains mock gateway, HTTP runner, and fixtures; never a product service |
| Helm chart | `charts/templiqx/` | Inventory-driven 8-Job conformance + mock gateway proof |
| Compose | `deploy/compose.yml` | Local adapter smoke |
| Supply chain / release | `scripts/supply-chain-smoke.sh`, `.github/workflows/release.yml` | SBOM/provenance, immutable digest and Cosign verification |

## Package trust handoff

Before handing a package to a host, export its canonical identity and run strict
verification:

```sh
templiqx --root packages export-package-identity crm3
export TEMPLIQX_PACKAGE_SIGNING_KEY="<local-or-ci-secret>"
templiqx --root packages sign-package crm3 --key-id ci \
  --expected-fingerprint "<manifest-fingerprint-from-export>"
templiqx --root packages verify-package-trust crm3 --strict
```

The environment key and embedded `sha256-keyed` signature are development/CI
controls only. Hosts must not treat them as production publisher identity.
Production delivery separately verifies the OCI image digest and its keyless
Cosign signature/attestation. Do not put signing keys in package files, command
arguments, MCP requests, logs, receipts, or operation envelopes.

## Verification entrypoints

```sh
just verify          # fmt, clippy, tests, boundaries, CI gates, qlty
just verify-deploy   # Docker, Compose, kind, supply chain (needs Docker)
just fresh-clone     # isolated worktree + empty Cargo cache
```

## Authorized merge context

Packages may declare `provenance.requires_authorized_context: "true"`. The host
validator supplies an opaque `AuthorizedMergeContext` envelope on every
render, eval, and compatibility preflight that binds merge data to tenant/matter
scope:

| Field | Host-owned meaning |
|-------|-------------------|
| `scope_id` | Tenant/matter scope authorized for this operation |
| `policy_decision_id` | Authorization decision identity |
| `policy_version` | Policy version bound to the decision |
| `evidence_provenance_id` | Retrieval/provenance identity for grounded facts |
| `issued_at` / `expires_at` | Freshness window |
| `fingerprint` | SHA-256 over canonical binding fields |

Inject the envelope in request `context` under `_templiqx_authorized_merge`.
The portable core fingerprints and binds the envelope but does not interpret
tenant policy. Missing, mismatched, expired, or redacted context fails closed
with stable diagnostics (`TQX_AUTHORIZED_CONTEXT_*`) before evaluation,
rendering, or a production-ready compatibility report.

Sanitized examples live under `examples/packages/*/fixtures/authorized-context.json`.
See [Cross-opco reference packages](../contracts/cross-opco-reference-packages-v1alpha1.md).

## Host-owned PDF conversion seam

PDF is **not** a default `TempliqxService` format selector. After the existing
DOCX or HTML artifact is produced, the host invokes an optional conversion
adapter that accepts a confined workspace artifact identity (source path, source
hash, declared output identity) and returns payload-free renderer metadata:
converter ID, version, environment identity, byte size, and output hash.

Minimum host contract:

- confined input/output paths under an ephemeral workspace
- no-network execution and least-privilege filesystem access
- resource limits, source-hash binding, output size/type checks
- cleanup on every outcome (success or failure)
- host-owned encrypted persistence, retention, and access controls

The repository records deterministic conversion fixture metadata for conformance;
it does not ship a default PDF converter in `templiqx-local`, CLI, or MCP.
See [Document conversion ADR](../adr/document-conversion.md) and
[Template compatibility report](../contracts/template-compatibility-report-v1alpha1.md).

## Explicitly host-blocked

- Real ModelGateway adapter implementation (Basenet host repo)
- Tenant / auth / approval / retrieval policy
- Production fixture ingestion and legal sanitization
- Real second opco selection (R13)
- Schema-valid human-review extraction without host `bli-61` agreement (R15)

See [Pre-CRM3 readiness](pre-crm3-readiness.md) and
[Deployment boundary](../architecture/deployment.md) for core boundaries.
