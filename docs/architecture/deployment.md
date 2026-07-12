# Deployment boundary

Templiqx deploys as a portable contract compiler plus host-owned adapters. The
core crates stay provider-neutral and do not embed CRM3, Basenet, auth,
retrieval, workflow, queueing, tenant policy, approval policy or secrets.

## Core Boundary

- `templiqx-contracts` owns stable serializable DTOs.
- `templiqx-core` parses, validates and compiles one interaction.
- `templiqx-ports` defines host adapter traits.
- `templiqx-application` exposes actor-neutral operations.
- `templiqx-mock` is conformance-only and deterministic.

Auth, tenant checks, approval, idempotency, retries, retrieval and workflow are
host policies around these operations. The conformance host harness proves those
policies can reject before runtime execution without adding them to core.

## CRM3 Conformance Deployment

The checked-in CRM3 scenarios under `examples/crm3/scenarios/**` are synthetic
fixtures. They are data-driven through `templiqx.mock/v1alpha1`, execute with an
injected virtual clock, and emit fingerprints or diagnostics only. Runtime
delays and retry-after values are simulated without sleeping.

Production deployment should provide real package stores, runtime adapters,
document adapters and host policy at the edge. The same Templiqx service methods
must remain the semantic boundary for humans and agents.

## Supply chain

CI builds the CLI image with BuildKit provenance and SBOM attestation. Consumers
verify artifacts with:

```sh
./scripts/supply-chain-smoke.sh
```

The smoke script asserts SBOM generation, Grype high/critical gate, and (in CI)
`artifacts/supply-chain/build-metadata.json` plus `provenance.json` linkage.
Package manifest signing is documented in
[`adr-package-trust.md`](adr-package-trust.md). Product-direction seams
(tool-contract refs, streaming port, observability) are in
[`adr-tool-contract-refs.md`](adr-tool-contract-refs.md),
[`adr-streaming-runtime-port.md`](adr-streaming-runtime-port.md), and
[`observability.md`](observability.md).

Host integration procedures: [`../guides/host-integration.md`](../guides/host-integration.md).

