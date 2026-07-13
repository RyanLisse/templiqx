---
date: 2026-07-13
type: log
status: informational
related_plans:
  - docs/plans/2026-07-12-001-feat-template-engine-parity-plan.md
  - docs/plans/2026-07-12-002-feat-agent-native-parity-plan.md
  - docs/plans/2026-07-13-001-feat-production-release-and-conformance-plan.md
---

# Deferred and host-blocked work log

This log separates work now implemented in the Templiqx repository from work
that cannot truthfully be completed without a release-tag run or the Basenet
CRM3 host. It supersedes the earlier note that package trust and the expanded
DOCX corpus were deferred.

## Closed in the production-readiness branch

| Previously deferred item | Current repository evidence |
|--------------------------|-----------------------------|
| Package trust round trip | Canonical identity export, CAS signing, strict verification, tamper/replay/duplicate/unsupported-algorithm tests |
| OCI distribution trust | Separate tag-gated Cosign signing and pulled-digest verification in `.github/workflows/release.yml` |
| Expanded DOCX corpus | Deterministic generator plus V1/V2 detection, supported V5 cases, unresolved-data behavior, and hostile ZIP fixtures under `examples/legacy-corpus/` |
| Agent-native lifecycle gaps | `update_package`, `delete_package`, and `delete_workspace_artifact`, all included in 26-operation behavior parity |
| MCP bootstrap gaps | Explicit workspace root/resource and `bootstrap` / `run-eval` prompts |
| Mock coverage gap | Strict 8-scenario inventory exercised in-process and through the HTTP gateway/deployment smoke paths |

The DOCX corpus is a measured synthetic compatibility surface, not a claim of
general DOCX support. The `sha256-keyed` package signature is a local/CI
tamper-evidence contract, not a production public-key identity. OCI digest
trust remains separately enforced by the release workflow.

## Pending external evidence

### First immutable release run

The repository contains the release workflow, validation scripts, three-image
artifact split, multi-platform definition, BuildKit SBOM/provenance settings,
Cosign verification, and chart packaging/checksums. Publication is not complete
until normal CI and a tag-triggered workflow succeed against GHCR and GitHub
Release. Record the resulting immutable digests and Sigstore bundles; do not
promote mutable tags alone.

### CRM3 host integration

The following remain explicitly host-blocked and are not Templiqx repository
defects:

- real Basenet ModelGateway `RuntimeAdapter` wiring;
- tenant, authentication, authorization, retrieval, approval, audit, retry,
  and workflow policy;
- sanitized production-customer fixture ingestion and legal acceptance;
- production storage/secrets/network controls and deployment acceptance;
- selection and validation of a real second opco package;
- final BLI-61/BLI-62 host schema and human-review agreement.

Host teams should run the same 8-scenario inventory and fingerprint contract
against their adapter, preserving evidence grounding and failure semantics.
See [Host integration](../guides/host-integration.md).

## Release claim boundary

Once repository verification and the tag workflow are green, the valid claim
is: **Templiqx-owned standalone compiler, packaging, and synthetic conformance
artifacts are release-ready.** It is not: **CRM3 is production-ready.**
