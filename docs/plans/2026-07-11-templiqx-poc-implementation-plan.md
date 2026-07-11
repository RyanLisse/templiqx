---
date: 2026-07-11
status: completed
requirements: ../brainstorms/2026-07-11-templiqx-ai-native-template-engine-poc-requirements.md
---

# Templiqx POC Implementation Plan

## Ideal State

A clean checkout builds a standalone modular Rust workspace. The same canonical application capabilities are callable through Rust, CLI, and MCP. A deterministic CRM3 conformance package validates and compiles typed extraction and drafting contracts, runs them through a fake runtime, migrates and renders the supported V5 DOCX subset, and proves normalized-OOXML parity without importing CRM3 or provider implementations into Templiqx.

## Architecture

```text
contracts <- ports
    ^          ^
    |          |
   core    concrete adapters (fake runtime, V5 DOCX)
     \        /
     application
          |
         local composition
          |
      CLI / MCP / Rust
```

Rules:

- `contracts`, `ports`, and `core` perform no network access and import no provider, CRM3, MCP, or legacy Java implementation.
- All human/agent surfaces use one actor-neutral application capability layer and the same request/response DTOs.
- Production runtime integrations are host-owned adapters. Templiqx ships typed ports and conformance fakes.
- Legacy source dialect is explicit at import. Runtime rendering never content-sniffs.
- Checked-in CRM3 examples are synthetic and sanitized.

## Phases

### P0 — Workspace and contracts

- Scaffold workspace, formatting/lint/test commands, dependency-boundary verifier, and docs navigation.
- Define canonical DTOs, diagnostics, operation envelopes, ids, receipts, and deterministic fingerprints.
- Verify envelope snapshots and hashing stability.

### P1 — Contract semantics and compiler

- Parse a strict declarative YAML source into a typed structured node tree.
- Validate inputs/context/output schemas and bounded expressions/components.
- Render deterministically and compile a provider-neutral interaction with capability negotiation.
- Reject unknown fields, unsupported capabilities, unsafe expressions, missing variables, and invalid outputs with stable diagnostics.

### P2 — Canonical application, local store, and surfaces

- Implement root-confined filesystem package storage and compare-and-swap updates.
- Implement atomic discover, inspect, create/update, validate, compile, render, migrate, test, diff, and explain operations.
- Add CLI and MCP as thin facades over the same application service.
- Prove surface catalog and structured-output parity.

### P3 — Runtime port and deterministic fake

- Define runtime adapter descriptors/capabilities and execution receipts.
- Implement a fixture-driven fake adapter with output-schema validation.
- Keep live model/provider SDKs outside the workspace.

### P4 — V5 DOCX compatibility slice

- Support `$data.*`, ordinary Word MERGEFIELD, body/tables/headers/footers, aliases, and unresolved-field diagnostics for one explicit V5 fixture class.
- Detect but do not execute/migrate V1 BeanShell, V2, `$func.*`, or unsupported V5 behavior.
- Preserve untouched ZIP entries and emit a categorized migration report.
- Canonicalize selected OOXML, remove only versioned volatile fields, and provide structural parity diagnostics.

### P5 — CRM3 vertical conformance

- Add sanitized extraction and drafting contracts aligned with BLI-61 and BLI-62.
- Pass only schema-validated extraction output into drafting.
- Render the structured draft through the migrated V5 DOCX fixture aligned with BLI-11/BLI-34.
- Produce a payload-free trace receipt connecting all relevant fingerprints and reports.

### P6 — Verification and handoff

- Run formatting, Clippy with warnings denied, workspace tests, CLI smoke tests, MCP protocol/parity tests, dependency checks, and clean-checkout reproduction.
- Independently review implementation correctness, security boundaries, and requirements coverage.
- Document limitations and the host-owned CRM3 integration seam.

## Verification Commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
just verify
```

The final verification must additionally exercise CLI JSON output, MCP stdio initialization/tool calls, the CRM3 conformance flow, V5 migration reporting, normalized-OOXML parity, path traversal rejection, and deterministic clean reruns.
