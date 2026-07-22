---
title: "feat: Deliver the quality evidence proposal protocol"
---

## Goal

Deliver one additive `assess_quality_proposals` operation that validates full
contract candidates and minimized host-attested evidence, enforces tiered hard
floors, and returns deterministic equal-objective Pareto fronts. Preserve the
host boundary and all existing eval and mutation semantics.

## Ideal state criteria

- Exact package, base contract, fixture set, candidate, policy, request, and
  report identities are computed and fail closed on mismatch.
- Claimed evaluator, scorer, model, and measurement identities are clearly
  labeled as host-attested and are never presented as authenticated truth.
- Logical trial cardinality, retry accounting, failure taxonomy, fixed-point
  aggregations, denominators, rounding, and overflow behavior match the
  [normative protocol](../contracts/quality-proposals-v1alpha1.md).
- Every scorer has one nonzero same-metric `gte` floor and Pareto objective.
  Data-driven `950_000` and `850_000` reference policies pass exact boundary
  tests, and floor failures never enter Pareto comparison.
- Rust, CLI, MCP, HTTP, OpenAPI, and all generated SDK surfaces are semantically
  equivalent.
- The operation returns no source or raw evaluation payload and performs no
  winner selection, mutation, persistence, publication, or promotion.
- No Ax, BAML, Effect, provider SDK, optimizer, or new host-policy dependency is
  introduced.

## Delivery slices

1. **Contracts and pure engine:** freeze DTO tags, limits, fingerprint domains,
   logical trial grid, checked aggregations, safety rules, diagnostics, ordering,
   and Pareto golden vectors.
2. **Canonical application operation:** bind current package/base/fixture
   identities, parse and validate full proposals, emit top-level proposal change
   paths, redact diagnostics, and prove assessment does not mutate.
3. **Transport and client parity:** add exactly one catalog, CLI, MCP, HTTP, and
   OpenAPI operation; move the Operations OpenAPI document to
   `1.0.0-alpha.2`, then use `just bump-engine` to classify and apply the
   additive bump (expected engine/SDK line `0.2.0`) and regenerate all five SDKs.
4. **Conformance and documentation:** ship sanitized high-consequence and
   general-advisory policies, dominated/incomparable/stale/infrastructure
   scenarios, this protocol, ADR, and host handoff.

## Boundaries

The host owns model calls, retries, evaluator execution, auth, signatures,
evidence retention, approval, candidate selection, and promotion. Templiqx owns
only deterministic assessment. Existing `put_contract` remains the separate
CAS-protected mutation path; `run_eval`, `test_package`, and `diff_contract`
remain compatible.

Ax is an external development-time optimizer, BAML a design reference only, and
Effect optional deferred host tooling. None enters Templiqx dependencies or the
production path.

Implementation starts in an empty isolated worktree created from the primary
checkout's recorded exact `HEAD`. No dirty primary-checkout file is copied, and
the final changed-path audit is limited to this protocol.

## Verification

Required evidence includes focused contracts/core/application/transport and
conformance tests; fingerprint, denominator, cardinality, boundary, redaction,
and permutation vectors; OpenAPI compatibility and strict five-SDK generation;
`cargo fmt`, workspace Clippy/tests, `./scripts/check-boundaries.sh`, strict docs
build, `just verify`, and `git diff --check`.

Any identity, denominator, privacy, parity, or failure-domain ambiguity blocks
completion. Stop after this one proposal-assessment operation; do not add an
optimizer, judge model, evidence store, apply endpoint, signature system, or
promotion operation.
