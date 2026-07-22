---
title: Quality evidence host integration
---

Use `assess_quality_proposals` as a deterministic checkpoint inside a host-owned
development or approval workflow. It is not an optimizer, model gateway,
evidence store, or promotion endpoint.

## Recommended flow

1. Load the current package and contract identities from Templiqx.
2. Select a data-driven quality policy. The reference policies use separate
   correctness and grounding floors of `950_000 ppm` for high-consequence work
   or `850_000 ppm` for general-advisory work.
3. Generate full candidate contract sources outside Templiqx. Candidate fixtures
   must remain identical to the base fixture set.
4. Execute the fixed fixture/replicate grid in the host. Retries are attempts
   within one logical trial, not new trials.
5. Evaluate every declared scorer and collect each resource objective for every
   logical trial. Keep provider errors classified as infrastructure failures;
   never turn them into successful or missing semantic rows.
6. Submit sanitized candidates plus minimized terminal evidence to
   `assess_quality_proposals`.
7. Reject ineligible candidates. Present nondominated candidates and their
   tradeoffs to host policy or a human approver; do not treat the first Pareto
   member as a winner.
8. After separate authorization and approval, apply the selected full contract
   through the existing CAS-protected `put_contract` path.

## Host responsibilities

The host owns:

- model/provider calls, timeouts, cancellation, budgets, and retry execution;
- evaluator and scorer execution;
- tokenizer, clock, billing, currency, and measurement definitions;
- authentication, tenant authorization, signatures, and evidence provenance;
- raw evidence storage and retention policy;
- optimizer execution, candidate selection, human approval, and promotion;
- customer-data controls before a candidate is attested synthetic or sanitized.

Templiqx owns structural validation, exact artifact bindings, cardinality,
checked integer aggregation, hard-floor eligibility, deterministic Pareto
fronts, redaction, and report fingerprints. Claimed evaluator, scorer, model,
and measurement profile fingerprints are consistency-checked opaque claims;
Templiqx does not authenticate them.

## Trial collection rules

- Produce exactly one terminal record for each
  `(fixture_id, replicate_index)` in the policy grid.
- Set `provider_attempt_count` to the total attempts for that logical trial.
- Resource observations include all attempts and are present even when the
  terminal outcome is an infrastructure failure.
- A scored or candidate-quality-failure trial partitions every declared scorer
  into passed and failed sets. An infrastructure failure has no scorer outcome.
- Do not omit bad, slow, costly, or failed trials. Cardinality, semantic
  coverage, and infrastructure gates intentionally fail closed.

Currency objectives fix an uppercase ISO-4217 code; token objectives fix
`prompt`, `completion`, or `total`; latency objectives fix one claimed
measurement-profile fingerprint. Every candidate must use the same declared
profiles so comparisons are structurally meaningful.

## Safety and selection

All scorer floors, minimum semantic cases, and maximum infrastructure failure
rules run before Pareto ranking. The portable engine does not hard-code the
reference tier names or values; hosts version and select policies as data.

Pareto fronts are an explanation of tradeoffs, not an approval. A host must
make selection policy explicit, preserve the full contract source outside the
assessment result, and re-check the current base identity before mutation.
Assessment itself performs no write, persistence, publication, or promotion.

## Tooling boundary

An external development-time Ax runner may propose contracts and use the
assessment report as feedback. BAML may be consulted as a design reference for
typed prompt contracts. Effect may be considered later for optional host-side
workflow orchestration. Do not add any of these to Templiqx manifests, ports,
the production runtime graph, or the model execution path.

## Data minimization

Submit only synthetic or sanitized candidate sources and minimized evidence.
Do not place prompts, messages, fixture inputs, model outputs, credentials, or
provider response bodies in profile IDs or other free-form identifiers.
Templiqx does not echo source or raw payload, but the host remains responsible
for safe request construction and raw-evidence retention.

## Integration checks

- Replaying evidence after any package, base contract, fixture, candidate, or
  policy change fails binding.
- `950_000` and `850_000` pass their matching `gte` floors; one PPM below fails.
- A faster or cheaper floor-failing proposal never appears on a Pareto front.
- Timeout-heavy evidence fails coverage or infrastructure policy rather than
  producing an inflated semantic score.
- Reordering candidates, trials, metrics, or rules does not change report bytes
  or fingerprints.
- Candidate source and raw payload canaries do not occur in the response.
- Package and contract identities remain unchanged before and after assessment.

See the [normative protocol](../contracts/quality-proposals-v1alpha1.md) and
[proposed ADR](../adr/quality-evidence-protocol.md).
