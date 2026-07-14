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

The deployable artifacts are intentionally split:

- `templiqx-cli`: minimal standalone compiler product image;
- `templiqx-mcp`: minimal stdio MCP product image;
- `templiqx-conformance`: synthetic fixtures, HTTP runner, and mock gateway.

Boundary and image-content checks keep mocks out of the two product images. The
Compose and Helm/kind paths enumerate all 8 entries from the scenario inventory;
the chart renders one Job per scenario. The conformance image and chart are test
artifacts, not a CRM3 production service.

## Supply chain

Repository CI checks local SBOM/digest policy with:

```sh
./scripts/supply-chain-smoke.sh
```

The smoke script asserts SBOM generation, Grype high/critical policy, and (in
CI) `artifacts/supply-chain/build-metadata.json` plus `provenance.json` linkage.
The tag-gated release workflow separately builds all three OCI targets for
linux/amd64 and linux/arm64 with BuildKit SBOM and max-mode provenance, signs
and verifies the pulled immutable digests with keyless Cosign, pins the
conformance chart to that verified digest, and packages, checksums, signs, and
verifies the Helm chart before creating a GitHub Release.
Until that workflow succeeds for a tag, the repository contains a verified
release definition rather than a published-release claim. See
[`../guides/releasing.md`](../guides/releasing.md).

Package manifest signing is documented in
[`adr-package-trust.md`](adr-package-trust.md). Product-direction seams
(tool-contract refs, streaming port, observability) are in
[`adr-tool-contract-refs.md`](adr-tool-contract-refs.md),
[`adr-streaming-runtime-port.md`](adr-streaming-runtime-port.md), and
[`observability.md`](observability.md).

Host integration procedures: [`../guides/host-integration.md`](../guides/host-integration.md).
