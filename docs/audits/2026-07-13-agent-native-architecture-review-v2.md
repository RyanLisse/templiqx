---
title: Agent-Native Architecture Re-Audit (v2) — Templiqx
---

**Date:** 2026-07-13
**Scope:** current production-readiness branch; application service, CLI, MCP,
filesystem composition, and conformance harness
**Baseline:** [v1 audit (2026-07-12)](2026-07-12-agent-native-architecture-review.md)
— **66% overall**
**Current score: 94%** (strong agent-native contract surface; remaining ceiling
is mostly intentional host/UI scope)

## Score summary

| Core principle | Score | % | Evidence-backed status |
|----------------|-------|---|------------------------|
| Action parity | 26/26 | 100% | Every canonical operation has Rust, CLI, and MCP behavior coverage |
| Tools as primitives | 25/26 | 96% | `test_package` intentionally remains a workflow convenience over eval primitives |
| Context injection | 6/6 | 100% | Catalog, packages, workspace resource, roots, empty state, and prompts |
| Shared workspace | 2/2 | 100% | CLI and MCP use the same confined filesystem composition |
| CRUD completeness | 16/16 | 100% | Contract, package/manifest, and workspace-artifact lifecycle operations |
| UI integration | 4/5 | 80% | Structured envelopes and filesystem visibility; no live push/watch surface |
| Capability discovery | 7/7 | 100% | Catalog/resources/instructions plus `bootstrap` and `run-eval` prompts |
| Prompt-native features | 8/12 | 67% | Contract behavior is declarative; deterministic compiler and host policy remain code/host concerns |

Weighted total: **94/100**. This is a current architecture score, not a claim
that CRM3 production integration is complete.

## What changed since the earlier 86% snapshot

### 26-operation parity

`templiqx_application::CAPABILITY_CATALOG` is the authority. CLI and MCP expose
the same 26 names, including package identity/sign/verify, package update and
delete, and workspace-artifact delete. The catalog-derived conformance test in
`crates/templiqx-conformance/tests/crm3.rs` fails when an operation lacks a
Rust/CLI/MCP behavior case. This replaces name-only parity with behavior parity.

`test_package` is the only workflow-lite operation. Agents can instead compose
the primitive `list_evals` and `run_eval` operations, so retaining the wrapper
does not create a separate semantic path.

### Complete repository-owned lifecycle

| Entity | Create | Read | Update | Delete |
|--------|--------|------|--------|--------|
| Contract | `put_contract` | `inspect_contract` | `put_contract` with CAS | `delete_contract` |
| Package / manifest | `create_package` | `discover_packages`, identity export | `update_package` with CAS | `delete_package` |
| Workspace artifact | render/execute writes | list/read | confined overwrite | `delete_workspace_artifact` with CAS |

Mutations preserve path/symlink confinement, compare-and-swap behavior,
signature invalidation, and dependency/untracked-content guards. Package-local
`sha256-keyed` trust is a development/CI tamper-evidence mechanism; it is not
the production OCI trust root.

### Agent context and discovery

The MCP surface provides `templiqx://catalog`, `templiqx://packages`, and
`templiqx://workspace`, dynamic root/workspace instructions, empty-state
guidance, and `bootstrap` / `run-eval` prompts. The MCP binary accepts explicit
package and workspace roots, so agents and humans operate on the same files
without a hidden agent sandbox.

## Remaining architectural ceiling

The remaining six points are not missing catalog operations:

- `test_package` is deliberately a convenience workflow rather than a single
  primitive;
- stdio MCP is stateless and has no push/subscription, recent-activity, or live
  watch UI;
- preferences, approval, authorization, retrieval, tenant context, and trace
  composition are host-owned;
- validation/compilation, DOCX safety, and deterministic rendering correctly
  remain code, while authorable interaction/eval behavior remains in contracts.

Adding a web UI, session store, or CRM3 policy to chase a literal 100% would
weaken the current boundary. A future host may add live resource subscriptions
or activity context without duplicating Templiqx semantics.

## Production-readiness relationship

Agent-native readiness and CRM3 production readiness are separate claims.
Templiqx can own and release the standalone 26-operation compiler surfaces,
package trust contract, deterministic DOCX compatibility corpus, and 8/8
synthetic scenario/conformance artifacts. A real CRM3 ModelGateway,
tenant/auth/retrieval/approval/audit policy, sanitized customer data, and host
deployment acceptance remain blocked on the Basenet host.

## Verdict

| Question | Answer |
|----------|--------|
| Current agent-native score | **94%** (up from 66% v1 and the earlier 86% v2 snapshot) |
| Rust/CLI/MCP action parity | **26/26 with catalog-derived behavior coverage** |
| Repository-owned CRUD | **Complete for contracts, packages/manifests, and workspace artifacts** |
| Literal 100% desirable? | **No** — remaining scope is workflow convenience or intentional host/UI ownership |
| CRM3 production-ready? | **Not claimed** — standalone release/conformance readiness is distinct from host integration |

## Related artifacts

- [Capability map](../architecture/capability-map.md)
- [Host integration guide](../guides/host-integration.md)
- [Pre-CRM3 readiness](../guides/pre-crm3-readiness.md)
- [Release procedure](../guides/releasing.md)
- [Production release and conformance plan](../plans/2026-07-13-001-feat-production-release-and-conformance-plan.md)
