---
title: Quality proposal assessment v1alpha1
---

`assess_quality_proposals` is an additive, proposal-only protocol for comparing
full contract candidates against host-attested evaluation evidence. Templiqx
validates the protocol, binds evidence to the current artifacts, applies safety
floors, and computes deterministic Pareto fronts. It does not call a model,
choose a winner, write a contract, persist evidence, or promote a candidate.

The result is evidence about a proposal, not proof that an evaluator, model,
clock, tokenizer, or billing system told the truth.

## Immutable scope

- The current base contract's `evals` are the fixture universe. Fixture IDs are
  unique, and every candidate must contain the canonically identical fixture set.
- A request contains 1 through 32 full candidate sources. Templiqx parses and
  validates each source and computes its semantic contract fingerprint.
- Candidate source must be attested synthetic or sanitized. Source is accepted
  only for validation and fingerprinting; it is never returned.
- Existing `run_eval`, `test_package`, `EvalFixture`, and `diff_contract`
  semantics are unchanged. Assessment emits a separate, sorted
  `proposal_change_paths` list. Its complete vocabulary is `/api_version`,
  `/id`, `/version`, `/description`, `/inputs`, `/context`, `/capabilities`,
  `/messages`, `/output_schema`, `/runtime_policy`, `/extensions`,
  `/components`, `/provenance`, and `/evals`. A changed `/evals` is reported but
  also makes the candidate invalid.

## Logical trials and complete evidence

The policy fixes `replicates_per_fixture` in `1..=20`. A candidate submits
exactly one terminal trial for every unique pair:

```text
(fixture_id, replicate_index), where replicate_index is 0..replicates_per_fixture-1
```

Missing, extra, or duplicate pairs make the candidate ineligible. Total trials
must not exceed 10,240 per candidate. Provider retries do not add logical trials:
`provider_attempt_count` is at least one, and resource observations include all
attempts made for that logical trial.

For every semantic trial, `passed_scorers` and `failed_scorers` are disjoint and
their union is exactly the policy's scorer set. A candidate-quality failure must
fail at least one scorer. For every logical trial, including infrastructure
failures, there is exactly one observation for every declared resource
objective. Selective scorer or resource omission is invalid.

Trial outcomes are separated into:

- `scored`;
- `candidate_quality_failure` (`schema`, `assertion`, or `invalid_output`);
- `infrastructure_failure` (`transport`, `timeout`, `rate_limit`,
  `provider_unavailable`, `provider_internal`, `cancellation`, `budget`, or
  `evaluator_infrastructure`).

Candidate-quality failures remain in semantic denominators. Infrastructure
failures have no scorer outcome, but remain in total-trial resource aggregates,
semantic coverage, and the infrastructure-failure gate. Excluding a failed
provider call therefore cannot improve a candidate's quality ratio.

## Integer aggregation

All public metrics are integers: ratios use parts per million (PPM), latency
uses milliseconds, tokens use counts, and cost uses currency microunits. Every
public quality integer is encoded as a JSON integer with OpenAPI `format: int64`
and is capped at `9_007_199_254_740_991` (`Number.MAX_SAFE_INTEGER`), with
smaller protocol-specific maxima where applicable. This makes integer values
lossless across all five SDKs, including TypeScript `number`. Checked `u128`
intermediates must fit that public ceiling; exceeding it is invalid evidence
with `TQX_QUALITY_LIMIT_EXCEEDED` and never saturates.

| Aggregation | Inputs | Result | Rounding |
| --- | --- | --- | --- |
| `binary_ratio_ppm` | One outcome for a scorer on each semantic trial | `passed * 1_000_000 / (scored + candidate_quality_failure)` | Floor |
| `mean` | One resource value per logical trial, including infrastructure trials | `sum / total_logical_trials` | Floor |
| `sum` | One resource value per logical trial, including infrastructure trials | Exact checked sum | Exact |
| `p95_nearest_rank` | One resource value per logical trial, sorted ascending | Value at zero-based index `((95*n + 99)/100) - 1` | Exact observed value |

No numeric zero is fabricated for a candidate-quality failure. Its failed scorer
contributes to the denominator and not to the pass numerator. A zero semantic
denominator is always ineligible.

The fixed coverage measures are:

```text
infrastructure_failure_ppm = infrastructure_trials * 1_000_000 / total_trials
semantic_coverage_ppm       = semantic_trials       * 1_000_000 / total_trials
```

Both use floor division. Policy gates `minimum_semantic_cases` and
`maximum_infrastructure_failure_ppm` before ranking.

## Non-vacuous safety floors

A valid policy declares at least one binary scorer. For every scorer it declares:

1. exactly one derived `binary_ratio_ppm` metric with `ratio_ppm` units and
   `maximize` direction;
2. exactly one hard rule on that same metric, using `gte` and a threshold in
   `1..=1_000_000`;
3. that scorer metric as a Pareto objective.

`gte 0`, `lte`, duplicate rules, a rule for another metric, a wrong unit, or a
missing scorer objective makes the policy invalid. Resource and budget rules may
use compatible `gte` or `lte` comparisons.

The shipped reference policies express their tiers as data, not domain enums:

| Reference policy | Correctness floor | Grounding floor |
| --- | ---: | ---: |
| High consequence | `950_000 ppm` | `950_000 ppm` |
| General advisory | `850_000 ppm` | `850_000 ppm` |

Correctness and grounding are separate scorer IDs, ratios, rules, and Pareto
objectives. The portable core does not encode legal, Wft, or other product-domain
tier names. A value equal to a floor passes; one PPM below it fails.

## Equal multi-objective comparison

All eligibility, coverage, infrastructure, and safety rules run before Pareto
comparison. An ineligible candidate can never appear on a front, even if it is
faster or cheaper.

Among eligible candidates, A dominates B only when A is no worse on every
objective and strictly better on at least one, respecting each objective's
`maximize` or `minimize` direction. There are no weights and no scalar winner.
Equal and incomparable candidates remain on the fronts. Zero eligible candidates
is a valid report with empty fronts; envelope `ok` describes protocol validity,
not candidate eligibility.

## Computed and claimed identities

The report deliberately separates identities Templiqx computes from opaque host
claims:

| Identity | Source | Classification |
| --- | --- | --- |
| Package | Current exported `PackageIdentity`, including artifact fingerprints | Computed |
| Base contract | Current parsed full contract | Computed |
| Fixture set | Current base `evals`, sorted by fixture ID | Computed |
| Candidate contract | Parsed full candidate source | Computed |
| Quality policy | Validated normalized policy | Computed |
| Request and report | Normalized protocol payloads | Computed |
| Evaluator and scorer profiles | Host request and evidence | Claimed, syntax/consistency checked only |
| Model profile | Host request and evidence | Claimed, syntax/consistency checked only |
| Measurement profile | Host request and evidence | Claimed, syntax/consistency checked only |

Templiqx compares expected and claimed values fail closed, but it does not
authenticate claimed profiles. Currency additionally fixes one uppercase
ISO-4217 code; tokens fix `prompt`, `completion`, or `total`; latency fixes a
claimed measurement profile describing timing and retry inclusion. These checks
establish comparability, not truth.

Protocol v1 freezes its ISO-4217 membership snapshot rather than depending on a
mutable runtime registry:

- source: [SIX Group data standards](https://www.six-group.com/en/products-services/financial-information/data-standards.html);
- snapshot date: `2026-07-22`;
- member count: `179` codes;
- SHA-256: `4c64388da43ddb82dbf818d7f68a5f8511da6a46bf2ef318d28d9e36eea0a8a7`,
  computed over the sorted uppercase codes joined by LF (`0x0A`) with no
  trailing LF.

Changing that snapshot is a protocol change and requires new compatibility
evidence; hosts must not silently substitute a live ISO table.

## Canonical fingerprints

SHA-256 fingerprints use canonical JSON and distinct domains:

| Domain | Canonical payload |
| --- | --- |
| `templiqx-package-identity/v1\0` | Exported `PackageIdentity` |
| Existing contract fingerprint | Full base or candidate `Contract`, including `evals` |
| `templiqx-quality-fixtures/v1\0` | Fixtures sorted by ID |
| `templiqx-quality-policy/v1\0` | Policy with scorers, rules, and objectives sorted by stable ID |
| `templiqx-quality-request/v1\0` | Normalized request without source bodies and with computed candidate fingerprints |
| `templiqx-quality-report/v1\0` | Complete report payload excluding only `report_fingerprint` |

Report arrays have canonical ordering: diagnostics by `(code,path)`, gates by
rule ID, candidates and front members by candidate fingerprint, aggregates by
metric ID, trials by `(fixture_id,replicate_index)`, and proposal change paths
lexically. Input permutations must produce identical report bytes and
fingerprints.

## Bounds and minimized output

The canonical service enforces the same post-deserialization limits for every
transport: 4 MiB normalized request, 512 KiB per candidate source, 512 fixtures,
20 replicates, 16 scorers, 16 objectives, 16 observations per trial, and
128-byte ASCII IDs using `[A-Za-z0-9._:-]`. HTTP also applies a 4 MiB body limit.

Diagnostics are sorted and deduplicated by `(code,path)`. Up to 256 are returned
unchanged. For 257 or more, the result contains the first 255 plus one final
`TQX_QUALITY_DIAGNOSTICS_TRUNCATED` diagnostic at `/`. Candidate validity never
changes because diagnostics were truncated.

Reports contain bounded identifiers, enums, fingerprints, counts, aggregates,
gate outcomes, trial summaries, and top-level change paths only. They never
contain candidate source, fixture inputs, prompts, messages, context, outputs,
credentials, provider error bodies, parser excerpts, or source snippets.
If identity claims fail protocol validation, `claimed_identities` is omitted.
If any trial field fails structural, identifier, profile, or public-integer
validation, all `trial_summaries` for that candidate are omitted rather than
reflecting invalid evidence into the public report.

## Ownership boundary

Templiqx owns deterministic validation, identity binding, aggregation,
eligibility, Pareto calculation, and report fingerprinting. The host owns model
calls, retry execution, evaluator execution, authentication, signatures,
authorization, evidence retention, approval, selection, and promotion. After
approval, a host may invoke the existing CAS-protected `put_contract` as a
separate mutation.

Ax may consume this operation as an external development-time optimizer. BAML
is a design reference only. Effect is optional, deferred host-side tooling.
None is a Templiqx dependency, provider adapter, runtime port, or production
execution path.

See [host integration](../guides/quality-evidence-host-integration.md) and the
[proposed ADR](../adr/quality-evidence-protocol.md).
