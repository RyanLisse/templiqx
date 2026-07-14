---
title: Architecture decisions
description: Index of Templiqx Architecture Decision Records (ADRs) — one record per significant, hard-to-reverse design choice, with its status and rationale.
---

# Architecture decisions

Architecture Decision Records (ADRs) capture the significant, hard-to-reverse design
choices behind Templiqx: what was decided, why, the alternatives weighed, and the
current status. Each record is immutable once accepted — a later decision supersedes
an earlier one rather than editing it. These are kept separate from the
[architecture](../architecture/poc) pages, which describe the system as it *is*; ADRs
explain *why* it is that way.

| ADR | Status | Summary |
| --- | --- | --- |
| [Package trust v1](package-trust) | Accepted (2026-07-12) | Manifest signing model and the host-owned key handoff |
| [Tool-contract references](tool-contract-refs) | Accepted (2026-07-12, design only) | Package-level `tool_contracts` table with compile-time `$ref` resolution |
| [Streaming `RuntimeAdapter` port](streaming-runtime-port) | Implemented (2026-07-12) | `execute_streaming` port method, `StreamEvent` contracts, deterministic mock replay |
| [ODT compatibility](odt-compatibility) | Accepted (2026-07-13, detect-only) | OpenDocument Text detection and migration scope; no render adapter in this slice |

## Conventions

- **One file per decision**, named for the decision (`package-trust.md`), not for a ticket.
- **Status is explicit**: `Proposed` → `Accepted` / `Implemented` → `Superseded by …`.
- **Scope is stated up front** — several ADRs here are *design-only* (accepted direction,
  implementation deferred). The status line says so.

See the [POC architecture](../architecture/poc) and [capability map](../architecture/capability-map)
for the current system these decisions produced.
