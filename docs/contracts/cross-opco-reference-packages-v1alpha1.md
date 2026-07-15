---
title: Cross-opco reference packages v1alpha1
---

Three sanitized, manifest-valid reference packages prove that one
`templiqx/v1alpha1` contract model can serve Legal/Basenet, regulated advice,
and Simplicate-shaped project-to-invoice workflows. Each package is synthetic,
contains no production credentials or customer payloads, and runs through the
same `TempliqxService` operations as CRM3.

## Package inventory

| Package ID | Path | Domain | Classification |
|------------|------|--------|----------------|
| `basenet-legal` | `examples/packages/basenet-legal/` | `legal/basenet` | synthetic |
| `finly-advice` | `examples/packages/finly-advice/` | `regulated-advice` | synthetic |
| `simplicate-workflow` | `examples/packages/simplicate-workflow/` | `project-invoicing` | synthetic |

Each manifest declares `api_version: templiqx/v1alpha1`, contract IDs, inline
eval request/output pairs, and package-local fixtures. Domain ownership remains
a host sign-off dependency before calling any package production evidence.

## Channel and domain matrix

| Package | Domain evidence | Channel / output proof | Eval fixture IDs |
|---------|-----------------|------------------------|------------------|
| `basenet-legal` | matter, parties, custom fields, financials, evidence | letter summary, safe HTML email, DOCX V5 template | `legal-extraction-request`, `legal-extraction-output`, `legal-draft-request`, `legal-draft-output` |
| `finly-advice` | regulated advice facts, suitability evidence | advice memo/report, safe email | `advice-extraction-request`, `advice-extraction-output`, `advice-memo-request`, `advice-memo-output` |
| `simplicate-workflow` | project, hours, rates, invoice lines | invoice draft, report summary, safe email, SMS notification | `hours-extraction-request`, `hours-extraction-output`, `invoice-draft-request`, `invoice-draft-output` |

Across the three packages the channel matrix covers safe HTML/plain email, memo,
SMS, report, invoice, and DOCX using the same approved merge-data semantics.
PDF is not a default `TempliqxService` format selector; hosts invoke the typed
conversion seam documented in [Host integration](../guides/host-integration.md)
after DOCX or HTML artifacts are produced.

Insurance/mortgage, HR, and pure accountancy domains are **not** claimed until a
later package is added.

## Contracts and templates

### `basenet-legal`

| Contract ID | Role |
|-------------|------|
| `legal-matter-extraction` | Grounded extraction from sanitized source fragments |
| `legal-document-drafting` | Typed draft output with `merge_data` for document adapters |

Templates:

| Path | Dialect | Purpose |
|------|---------|---------|
| `templates/v5-legal-template.docx` | DOCX V5 | Legal letter / matter document |
| `templates/draft-email.html` | HTML/plain | Safe email draft |

Migration alias map: `migrations/v5-aliases.json`.

### `finly-advice`

| Contract ID | Role |
|-------------|------|
| `advice-fact-extraction` | Grounded regulated-advice fact extraction |
| `advice-memo-drafting` | Memo, summary, and safe email fields |

### `simplicate-workflow`

| Contract ID | Role |
|-------------|------|
| `project-hours-extraction` | Grounded project hours extraction |
| `invoice-drafting` | Invoice lines, totals, report summary, and SMS text |

## Authorized merge context

Packages that declare `provenance.requires_authorized_context: "true"` (currently
`basenet-legal`) require a host-supplied `AuthorizedMergeContext` envelope in
the render/eval request context under the key `_templiqx_authorized_merge`.

| Field | Type | Notes |
|-------|------|-------|
| `scope_id` | string | Host-owned tenant/matter scope |
| `policy_decision_id` | string | Host authorization decision identity |
| `policy_version` | string | Policy version bound to the decision |
| `evidence_provenance_id` | string | Host evidence provenance identity |
| `issued_at` | string | RFC 3339 issuance timestamp |
| `expires_at` | string | RFC 3339 expiry timestamp |
| `fingerprint` | string | SHA-256 over canonical binding fields |

The portable core fingerprints and binds this envelope but does not interpret
tenant policy. Missing, mismatched, expired, or redacted context fails closed
before evaluation or rendering.

Sanitized fixture IDs (for conformance and documentation only):

| Package | Fixture path | Scope ID |
|---------|--------------|----------|
| `basenet-legal` | `fixtures/authorized-context.json` | `SYN-LEGAL-SCOPE-001` |
| `finly-advice` | `fixtures/authorized-context.json` | `SYN-ADVICE-SCOPE-001` |
| `simplicate-workflow` | `fixtures/authorized-context.json` | `SYN-PROJECT-SCOPE-001` |

## Measured DOCX fixture IDs (Legal floor)

Claims are fixture-ID based. Only constructs backed by corpus fixtures may be
called supported in adapter documentation.

### Render-supported (bounded, same-part)

| Fixture ID | Construct | Corpus path |
|------------|-----------|-------------|
| `v5-legal-repeat-rendered` | `${#...}` / `${/...}` single-level table-row repeat | `examples/legacy-corpus/fixtures/v5-legal-repeat-rendered/` |
| `v5-legal-conditional-rendered` | `${?...}` / `${/...}` whole-paragraph conditional | `examples/legacy-corpus/fixtures/v5-legal-conditional-rendered/` |

### Detect-only (report, no render)

| Fixture ID | Construct | Corpus path |
|------------|-----------|-------------|
| `v5-repeat-marker-detected` | repeat markers | `examples/legacy-corpus/fixtures/v5-repeat-marker-detected/` |
| `v5-conditional-marker-detected` | conditional markers | `examples/legacy-corpus/fixtures/v5-conditional-marker-detected/` |
| `v1-beanshell-detected` | V1 BeanShell | `examples/legacy-corpus/fixtures/v1-beanshell-detected/` |
| `v2-marker-detected` | V2 `${v2:...}` markers | `examples/legacy-corpus/fixtures/v2-marker-detected/` |

Additional measured DOCX fixtures shared with CRM3: `v5-nested-table`,
`v5-header-footer`, `v5-alias-collision-missing`. See
`examples/legacy-corpus/README.md` and `adapters/templiqx-docx-v5/README.md`.

## Explicit escape hatch (deferred)

The following remain outside the measured floor and must not be implied by
reference-package conformance:

- XLSX, RTF, ODT, PPTX, and PDF/A compatibility
- Nested, cross-part, or split-region repeat/conditional regions
- Binary image insertion (detect-only until a separate fixture proves safe handling)
- Arbitrary helper syntax, BeanShell/V1 code, reflective query-anything
- Jinja/Handlebars/Velocity/Blade compatibility

Preflight and migration reporting use
[Template compatibility report v1alpha1](template-compatibility-report-v1alpha1.md).

## Verification

```sh
cargo test -p templiqx-conformance reference_package_claims
cargo test -p templiqx-application --test authorized_context
cargo test -p templiqx-conformance --test legal_docx
```

Synthetic proof is not production validation. Host teams must replace fixtures
with sanitized production data and supply real authorized context before claiming
production readiness.
