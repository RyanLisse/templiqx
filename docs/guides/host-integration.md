# Host integration guide

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
| Kubernetes runtime | OCI runner artifacts | Chart / Job / mock gateway | Host chart or sidecar integration |

## ModelGateway consumer contract

The host implements `RuntimeAdapter` (or HTTP transport compatible with
`templiqx-runtime-http-mock`) and runs the same scenario suite as the mock gateway.

### Scenario inventory

Scenarios live under `examples/crm3/scenarios/` with inventory at
`examples/crm3/scenarios/inventory.json`. Each scenario has a manifest under
`examples/crm3/scenarios/<id>/manifest.json`.

Key scenarios for adapter parity:

| Scenario | Purpose |
|----------|---------|
| `intake-document-01` | Happy-path extraction → draft → DOCX |
| `draft-with-citations` | Grounded drafting with evidence links |
| `invalid-output-schema` | Schema rejection before downstream steps |
| `ambiguous-date` | Typed runtime failure (no invented dates) |
| `missing-required-field` | Missing field handling |
| `docx-unresolved-reference` | DOCX merge-field edge case |

### HTTP conformance runner

Use `tools/templiqx-http-conformance` against the host adapter URL:

```sh
export TEMPLIQX_RUNTIME_URL=https://host-model-gateway.example/v1
export TEMPLIQX_RUNTIME_SCENARIO=intake-document-01
templiqx-http-conformance
```

Compare receipt fingerprints with golden `scripts/golden/http-conformance.json`
after normalizing JSON. The mock gateway (`tools/templiqx-mock-gateway`) is the
reference transport; it must not ship as a production adapter.

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
| Helm chart | `charts/templiqx/` | Conformance Job + mock gateway proof |
| Compose | `deploy/compose.yml` | Local adapter smoke |
| Supply chain | `scripts/supply-chain-smoke.sh` | SBOM, Grype, provenance verify |

## Verification entrypoints

```sh
just verify          # fmt, clippy, tests, boundaries, CI gates, qlty
just verify-deploy   # Docker, Compose, kind, supply chain (needs Docker)
just fresh-clone     # isolated worktree + empty Cargo cache
```

## Explicitly host-blocked

- Real ModelGateway adapter implementation (Basenet host repo)
- Tenant / auth / approval / retrieval policy
- Production fixture ingestion and legal sanitization
- Real second opco selection (R13)
- Schema-valid human-review extraction without host `bli-61` agreement (R15)

See [Pre-CRM3 readiness](pre-crm3-readiness.md) and
[Deployment boundary](../architecture/deployment.md) for core boundaries.
