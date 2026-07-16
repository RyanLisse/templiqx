---
title: "ADR: Track high-complexity Rust module refactors (no mass rewrite)"
---

## Status
Accepted (2026-07-16) — tracking decision; implementation deferred to the
sequenced plan.

## Context
`qlty smells --all` (2026-07-15 audit) flagged large `lib.rs` monoliths in
portable crates (`templiqx-core`, `templiqx-application`, `templiqx-docx-v5`,
and secondary crates). Phase 0 of the smell plan landed: exclude worktrees and
generated SDK sources so duplication noise no longer dominates the signal.

Mass refactors during concurrent feature work (SDK pilots, report-engine gaps)
risk behavior drift on CRM3 grounding and fail-closed validation.

## Decision

1. **Do not** perform large complexity rewrites as part of readiness cleanup.
2. **Track** splits in
   [`docs/plans/2026-07-15-002-chore-qlty-smells-refactor-optimize-plan.md`](../plans/2026-07-15-002-chore-qlty-smells-refactor-optimize-plan.md)
   with inventory in
   [`docs/audits/2026-07-15-qlty-smells-findings.md`](../audits/2026-07-15-qlty-smells-findings.md).
3. Keep `qlty smells` advisory (`[smells] mode = "comment"`). Gate on
   `qlty check --level=low` and Clippy `-D warnings` only.
4. Execute Phases 1–4 **one crate per PR**, behavior-preserving moves first,
   complexity helpers only with golden/conformance coverage.
5. Suppress remaining noise via `.qlty/qlty.toml` excludes (generated SDKs,
   worktrees, `scratchpad/`, `artifacts/`) rather than silencing real product
   complexity.

## Priority backlog (from plan)

| Phase | Target | Status |
| --- | --- | --- |
| 0 | Exclude worktrees + generated SDK noise | Landed |
| 1 | Split `crates/templiqx-core/src/lib.rs` | Deferred |
| 2 | Split `templiqx-application` / `validate_package` | Deferred |
| 3 | Split `adapters/templiqx-docx-v5` (fixture-gated) | Deferred |
| 4 | Secondary: local / mcp / mock / http | Deferred |
| 5 | Optional SDK ergonomics | Parked |

## Non-goals

- Failing CI on smell complexity thresholds before Phase 1–3 land.
- Rewriting DOCX dialect claims or relaxing `templiqx/v1alpha1` validation.
- Refactoring generated OpenAPI client sources.
