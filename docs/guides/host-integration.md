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
| Document rendering | Port + bounded adapters (DOCX V5, Typst, XLSX, RTF, Markdown, tabular) | Package fixtures | Storage, permissions, lifecycle |
| Document preflight (`inspect_document`) | Port + V5 adapter | Legacy corpus inspect fixtures | Host template storage only |
| PDF / conversion | ADR entry criteria + typed seam | Recorded fixture metadata only | Host-constructed converter adapter |
| Authorized merge context | Fingerprint + fail-closed binding | Synthetic `fixtures/authorized-context.json` | Host validator supplies scope, policy, provenance |
| Authorized query / schema introspect | `DataIntrospectPort` / `AuthorizedQueryPort` | `FakeDataAccess` + synthetic OData-shaped fixture | Host OData (or chosen query) + row-level `can()` |
| Translation bundle policy | Package artifacts + filters | Demo `translations/` | Tenant locale selection |
| Operations HTTP | `templiqx-http` router + OpenAPI | Deterministic-fake demo binary | Host auth/TLS + bind router (or host process) |
| Kubernetes runtime | Signed CLI/MCP + conformance images | Synthetic conformance image, 8 Jobs, mock gateway | Host chart or sidecar integration |

## Beyond synthetic CRM3 (in-repo enablement)

Synthetic CRM3 under `examples/crm3/` remains the language-neutral conformance
proof. This repository also ships host-facing seams so Basenet (and other opcos)
can progress without pulling host policy into portable core:

| In-repo seam | Location | Host still owns |
|--------------|----------|-----------------|
| Operations OpenAPI + pilot SDKs | `openapi/`, `sdk/*` | Auth tokens, base URL, retries |
| HTTP conformance gate | `just conformance-http` | Basenet ModelGateway certification (BLI-246 host side) |
| Authorized merge context | contracts + application binding | Tenant/matter policy issuance |
| Evidence fragment shape | `docs/contracts/evidence-fragment-v1alpha1.md` | Retrieval + legal sanitization |
| Merge-data / `customFields` | `docs/contracts/merge-data-v1alpha1.md` | Resolving display names before render |
| Data access ports | `crates/templiqx-ports/src/data_access.rs` | Real query surface + authorization |
| Report definitions | `docs/contracts/report-definition-v1alpha1.md` | Assembler workflow + persistence |
| Cross-opco package examples | `examples/packages/` (incl. `basenet-legal`) | Production data + second-opco acceptance |

**Honest remainder (host-only):** live ModelGateway wiring, tenant/auth/SSO,
retrieval authorization, approval/audit persistence, legal-policy review of
customer fixtures, and production promotion controls stay in the Basenet host
(see BLI-243 family). Do not import those concerns into
`templiqx-contracts` / `templiqx-core` / `templiqx-ports` vocabulary.

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
| CLI OCI image | `Dockerfile` target `templiqx-cli` | Sidecar or Job runner — **signed** on tag release |
| MCP OCI image | `Dockerfile` target `templiqx-mcp` | Agent stdio transport — **signed** on tag release |
| Synthetic conformance OCI image | `Dockerfile` target `templiqx-conformance` | Contains mock gateway, HTTP runner, and fixtures; never a product service — **signed** |
| Operations HTTP server image | `Dockerfile` target `templiqx-http-server` | **Local/demo only** (`TEMPLIQX_RUNTIME_MODE=deterministic-fake` by default). **Not** a signed release artifact; production hosts bind `templiqx_http::router` |
| Helm chart | `charts/templiqx/` | Inventory-driven 8-Job conformance + optional local HTTP demo |
| Compose | `deploy/compose.yml` | Local adapter smoke (HTTP demo + mock profile) |
| Supply chain / release | `scripts/supply-chain-smoke.sh`, `.github/workflows/release.yml` | SBOM/provenance, immutable digest and Cosign verification for the three signed images + chart |

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

## Report-engine assembler handoff (BLI-230)

When assembling merge context for report definitions and grounded contracts, the
host owns:

1. **Evidence fragments** — retrieve and authorize fragments, then pass the portable
   shape in [`evidence-fragment-v1alpha1`](../contracts/evidence-fragment-v1alpha1.md)
   (identity, UTF-8 offsets, `quote_sha256`, optional revision checksum).
2. **`customFields` resolution** — resolve display names for `relation_link` values
   **before** render so the template engine stays IO-free
   ([`merge-data-v1alpha1`](../contracts/merge-data-v1alpha1.md)).
3. **Authorized query port** — implement `DataIntrospectPort` / `AuthorizedQueryPort`
   behind host OData (or chosen query surface); Templiqx ships traits + synthetic
   fixtures only (`examples/packages/basenet-legal/fixtures/authorized-query-response.json`).
4. **Revision checksum on fragments** — bind DMS revision identity when available
   (BLI-68) so grounded drafts survive store mutations.

### Receipt fingerprint as `document_version.checksum` (R10)

The Templiqx artifact fingerprint (SHA-256) **is** the host document-store
checksum for generated reports: one `document_version` row per render, no
parallel report-receipt table. See
[Report engine compatibility](report-engine-compatibility.md).

### Host prerequisites (R12)

Tracked host dependencies Templiqx does **not** build:

| Prerequisite | Why it blocks production |
|--------------|--------------------------|
| `compileToFilter` (ADR-0002, unbuilt) | Row-level `can()` for every authorized query |
| `document_version` write-race fix | Required before Templiqx receipts persist as versions |
| AI authoring agent + hybrid loop + A/B routing | Host UI/workflow; Templiqx supplies validate/compile/explain/diff |
| Query interface choice (OData / GraphQL / DSL) | Must settle before the query port hardens |

Format coverage and non-claims live in
[Report engine compatibility](report-engine-compatibility.md).
Legacy files under `examples/legacy-corpus/v5-report-templates/` evidence **formats only** (DOCX/RTF/BIFF
presence); they are Velocity-era binaries, not Templiqx definitions — do not treat them
as round-trip fixtures.
