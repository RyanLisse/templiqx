---
title: "Qlty smells audit — what to fix"
date: 2026-07-15
type: audit
---

# Qlty smells findings (2026-07-15)

> **Status:** informational · Phase 0 noise excludes landed · monolith splits
> deferred (tracked in ADR + plan) · **Source:** `qlty smells --all` (qlty 0.633.0) ·
> **Related plan:** `docs/plans/2026-07-15-002-chore-qlty-smells-refactor-optimize-plan.md` ·
> **Tracking ADR:** `docs/adr/high-complexity-rust-modules.md`

Inventory from `qlty smells --all` against the Templiqx workspace. This document
lists **what to fix**; the sequenced remediation lives in the related plan.
No code changes are proposed here.

## Method

```bash
qlty smells --all
```

- Analyzed **163 files** (structure + duplication).
- Parsed **508** smell events total; **255** were attributed to
  `.worktrees/feat-cross-opco-breadth-impl/**` (noise).
- After filtering worktree paths: **253** main-tree events.
- After also excluding generated OpenAPI client
  (`sdk/dotnet/**/Generated/**`): the actionable set below.

## Critical measurement caveat

**All 131 main-tree “duplication” hits were false positives.** Every
`also found at` path pointed at the matching file under
`.worktrees/feat-cross-opco-breadth-impl/…` — i.e. git worktree clones of the
same sources, not real copy-paste inside the product tree.

Until `.worktrees/**` (and preferably generated SDK sources) are in
`.qlty/qlty.toml` `exclude_patterns`, `qlty smells --all` is not a reliable
duplication signal.

Smells tools that walk the filesystem treat worktrees as sibling source trees.
A clean checkout without `.worktrees/` would report ~0 duplication; the
complexity / parameter / nesting findings remain real.

## Headline totals (actionable)

| Smell kind | Count (excl. worktree + Generated) | Priority |
|------------|--------------------------------------|----------|
| Function high complexity | 18 | P0–P1 |
| File high complexity | 9 | P0 |
| Many parameters | ~20 Rust core/adapter (rest are SDK/tooling) | P1–P2 |
| Many returns | 7 | P2 |
| Complex boolean | 8 | P2 |
| Deep nesting (level ≥ 5) | 4 | P1 |
| Real intra-repo duplication | **0** (after worktree filter) | — |

## P0 — Monolithic files (complexity + size)

Repo file-size discipline targets &lt;500 lines (hard max ~500 in agent guidelines).
These `lib.rs` files concentrate almost all portable logic:

| File | Lines (approx.) | File complexity (qlty) | Notes |
|------|-----------------|------------------------|-------|
| `crates/templiqx-core/src/lib.rs` | 2007 | **309** | parse / validate / schema / render / compile / filters |
| `adapters/templiqx-docx-v5/src/lib.rs` | 1862 | **239** | ZIP + OOXML migrate/render/inspect |
| `crates/templiqx-application/src/lib.rs` | 1721 | **185** | entire `TempliqxService` surface |
| `crates/templiqx-local/src/lib.rs` | 1220 | 139 | FS composition + CAS writes |
| `crates/templiqx-mcp/src/lib.rs` | 1204 | (not in top file list) | MCP transport |
| `crates/templiqx-mock/src/lib.rs` | 1145 | 91 | scenario inventory validation |
| `crates/templiqx-http/src/lib.rs` | 1105 | 60 | operations HTTP surface |

Also noisy but secondary: `scripts/bump-engine-version.mjs` (complexity 99),
`scripts/openapi/validate.mjs` (52), `crates/templiqx-http/tests/openapi_drift.rs` (71).

## P0–P1 — Highest-complexity functions

| Complexity | Function | Location |
|------------|----------|----------|
| 50 | `validate_package` | `crates/templiqx-application/src/lib.rs` |
| 47 | `validate_nodes` | `crates/templiqx-core/src/lib.rs` |
| 43 | `validate_bounded_schema` | `crates/templiqx-core/src/lib.rs` |
| 36 | `validate` | `crates/templiqx-mock/src/lib.rs` |
| 33 | `validate_contract` | `crates/templiqx-core/src/lib.rs` |
| 28 | `semantic_sources` | `adapters/templiqx-docx-v5/src/lib.rs` |
| 25 | `render_complex_fields` | `adapters/templiqx-docx-v5/src/lib.rs` |
| 24 | `run` | `tools/templiqx-http-conformance/src/main.rs` |
| 24 | `createTempliqxClient` | `sdk/typescript/src/client.ts` |
| 23 | `load_inventory` | `crates/templiqx-mock/src/lib.rs` |
| 22 | `validate_component_cycles` | `crates/templiqx-core/src/lib.rs` |
| 21 | `put_contract` | `crates/templiqx-local/src/lib.rs` |
| 21 | `migrate_split_aliases` | `adapters/templiqx-docx-v5/src/lib.rs` |
| 20 | `validate_values` | `crates/templiqx-core/src/lib.rs` |
| 19 | `render_nodes` | `crates/templiqx-core/src/lib.rs` |
| 18 | `render_simple_fields` | `adapters/templiqx-docx-v5/src/lib.rs` |
| 18 | `component_arguments` | `crates/templiqx-core/src/lib.rs` |
| 18 | `binary_stdio_is_protocol_clean_and_serves_tools` | `crates/templiqx-mcp/tests/stdio.rs` |

## P1 — Deep nesting (level = 5)

| Location | Likely area |
|----------|-------------|
| `adapters/templiqx-docx-v5/src/lib.rs` | OOXML attribute / instruction walk (~line 557) |
| `crates/templiqx-application/src/lib.rs` | package validation loops |
| `crates/templiqx-core/src/lib.rs` | two sites in node/schema validation |

## P1–P2 — Many parameters (Rust product code)

Worth fixing with context structs (behavior-preserving):

| Params | Function | Location |
|--------|----------|----------|
| 7 | `apply_field` | `adapters/templiqx-docx-v5/src/lib.rs` |
| 6 | `render_text_group` | `adapters/templiqx-docx-v5/src/lib.rs` |
| 6 | `TempliqxService::new` | `crates/templiqx-application/src/lib.rs` |
| 6 | `execute_contract` | application + http |
| 6 | `validate_nodes` | `crates/templiqx-core/src/lib.rs` |

**Defer / accept:** hand-written .NET/Python SDK method parameter lists mirror
the operations HTTP API; fix only if introducing request DTOs at the OpenAPI
layer. Generated `OperationsV1.cs` should be **excluded** from smells, not
refactored.

## P2 — Many returns / complex booleans

| Function | Returns | Location |
|----------|---------|----------|
| `validate` | 15 | `crates/templiqx-mock/src/lib.rs` |
| `valid_date_time` | 9 | `crates/templiqx-core/src/lib.rs` |
| `put_contract` | 9 | `crates/templiqx-local/src/lib.rs` |
| `load_inventory` | 9 | `crates/templiqx-mock/src/lib.rs` |
| `validate_stream_events` | 9 | `crates/templiqx-mock/src/lib.rs` |
| `read_package` | 6 | `adapters/templiqx-docx-v5/src/lib.rs` |
| `execute` | 6 | `tools/templiqx-mock-gateway/src/main.rs` |

Complex booleans: DOCX instruction prefix checks, core validation predicates,
`scripts/bump-engine-version.mjs`, mock-gateway path sanitization.

## Noise to exclude (not fix)

| Path pattern | Why |
|--------------|-----|
| `.worktrees/**` | Duplicate checkouts inflate duplication + double every smell |
| `sdk/dotnet/**/Generated/**` | OpenAPI codegen; file complexity 1107 |
| `**/target/**`, `**/node_modules/**`, `**/dist/**` | Already excluded |
| Conformance scenario fixtures / large golden tests | Prefer `--include-tests` only when intentionally auditing tests |

## Refactor / optimize opportunities (beyond raw smells)

These are engineering targets suggested by the smell map, not separate qlty
rules:

1. **Module split (primary optimization)** — Break monolith `lib.rs` files into
   focused modules (`parse`, `validate`, `schema`, `render`, `compile`,
   `filters` for core; `service/{catalog,package,contract,document,eval}.rs`
   for application; `zip`, `migrate`, `render`, `inspect` for DOCX). Re-export
   public API from `lib.rs` so callers stay stable.
2. **Visitor / state machine for OOXML** — `semantic_sources`,
   `migrate_split_aliases`, `render_*_fields` share Reader/event loops; a
   small event visitor would cut complexity and nesting together.
3. **`validate_package` decomposition** — Extract per-check helpers
   (manifest, contracts, tool refs, fingerprints, evals) so the orchestrator
   stays linear.
4. **Request/context structs** — Collapse 6–7-arg helpers (`apply_field`,
   `validate_nodes`, `execute_contract`) without changing semantics.
5. **Do not chase micro-performance** until modules land — no evidence smells
   equate to CPU hotspots; prefer `tools/templiqx-bench` if a bottleneck appears
   after splits.

## Suggested acceptance bar (for the plan)

After remediation waves:

1. `.qlty/qlty.toml` excludes `.worktrees/**` and generated SDK sources.
2. Re-run `qlty smells --all`: **0 worktree-echo duplications**; generated
   client absent from report.
3. No single product `lib.rs` above ~500–600 lines without a tracked exception.
4. No P0 function above complexity ~25 without justification comment or further
   split.
5. `just verify` green; CRM3 conformance unchanged
   (`cargo test -p templiqx-conformance --test crm3`).

## Raw artifact

Full colored CLI capture was ~1MB; structured inventory was derived locally
during this audit (`/tmp/qlty-smells-inventory.json` on the author machine —
not committed). Re-generate with:

```bash
qlty smells --all --no-snippets | tee /tmp/qlty-smells-all.txt
```
