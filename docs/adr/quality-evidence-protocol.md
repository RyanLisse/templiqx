---
title: "ADR: Proposal-only quality evidence protocol"
---

## Status

Proposed (2026-07-22).

## Context

Hosts need to compare candidate AI interaction contracts on correctness,
grounding, cost, tokens, and latency without moving provider execution or
optimization into Templiqx. Extending `run_eval` would conflate deterministic
fixture evaluation with externally measured, host-attested evidence. A weighted
winner would also allow low cost or latency to hide an unsafe quality result.

External evidence is assertive, not self-authenticating. Templiqx can verify
structure, cardinality, immutable bindings, comparability, and deterministic
math, but cannot prove that a host evaluator, model, clock, tokenizer, or
billing source was truthful.

## Decision

Add one actor-neutral operation, `assess_quality_proposals`, across the canonical
service and thin transports.

1. Templiqx loads the current package and base contract, validates every full
   candidate source, computes package/contract/fixture/candidate/policy/request
   identities, and distinguishes them from claimed evaluator, scorer, model,
   and measurement profile identities.
2. The current base contract's fixtures define a fixed logical trial grid.
   Candidate-quality failures remain in semantic denominators; infrastructure
   failures remain in coverage, infrastructure, and resource calculations.
3. Integer-only, checked fixed-point aggregation follows the normative
   denominator and rounding rules in the
   [quality proposal contract](../contracts/quality-proposals-v1alpha1.md).
4. Every binary scorer has one nonzero same-metric `gte` ratio floor and is also
   a Pareto objective. All floors run before equal multi-objective Pareto
   comparison. The `950_000` and `850_000` reference floors are policy data,
   not hard-coded domain types.
5. The operation returns deterministic assessments and Pareto fronts. It never
   chooses a winner, mutates a package, persists evidence, publishes, applies,
   or promotes a candidate.
6. Existing `run_eval`, `test_package`, `EvalFixture`, and `diff_contract`
   behavior remains unchanged.

## Ownership

The host retains model calls, provider retries, optimizer and evaluator
execution, authentication, signatures, authorization, approval, evidence
retention, selection, and promotion. Templiqx structurally validates claimed
profiles but does not certify them.

Ax is permitted only as an external development-time optimizer. BAML informs
contract and prompt design but is not integrated. Effect may be evaluated later
as optional host orchestration tooling. This decision adds no Ax, BAML, Effect,
provider SDK, optimizer, or new host-policy port dependency.

## Consequences

- A cheap or fast proposal below any hard quality floor cannot be Pareto-ranked.
- Equal and incomparable eligible proposals remain visible; product policy must
  make the eventual selection explicitly.
- Reports are reproducible and bound to exact artifacts, while evidence
  authenticity remains a host trust decision.
- The public surface gains one additive operation and corresponding transport
  and SDK work, but no persistent migration or second runtime path.
- A valid operation may return no eligible candidates and still have
  `OperationEnvelope.ok = true`.

## Alternatives rejected

- **Extend `run_eval`:** mixes external attestation with existing deterministic
  fake-eval semantics.
- **Embed Ax, BAML, or Effect:** crosses the host boundary and adds runtime
  coupling without improving deterministic evidence semantics.
- **Weighted or automatic winner:** hides tradeoffs and can let resource metrics
  compensate for unsafe quality.
- **Store or promote from assessment:** creates authority and lifecycle semantics
  that belong to the host and bypasses the existing CAS mutation boundary.

## Follow-ups

Hosts may define authenticated evidence/signature policy, build an external Ax
runner, and expand sanitized benchmark corpora after the protocol is stable.
Those follow-ups do not alter this proposal-only operation.
