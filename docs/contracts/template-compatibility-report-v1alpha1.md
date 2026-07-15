---
title: Template compatibility report v1alpha1
---

Payload-free preflight and migration report for package-confined document
templates. The report records construct findings, unresolved fields, and output
readiness without document bodies, merge data, or host secrets.

## Operations

| Operation | When used | Mutates workspace |
|-----------|-----------|-------------------|
| `inspect_document` | Read-only preflight on an existing template | no |
| `migrate_legacy` | Import legacy source into a canonical package template | yes (canonical template path) |

Both operations return adapter-specific report data inside a portable envelope.
For DOCX V5 dialect analysis the nested `dialect_report` matches the adapter
`CompatibilityReport` shape documented in
[Document inspection v1alpha1](document-inspection-v1alpha1.md).

## Report envelope

Top-level fields apply to every dialect. Unknown fields are rejected
(`deny_unknown_fields`).

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `api_version` | string | yes | Must be `templiqx.template-compatibility/v1alpha1` |
| `source_template` | string | yes | Package-relative or legacy import path analyzed |
| `source_version` | string | no | Host or legacy template version label when known |
| `dialect` | string | yes | Explicit adapter dialect (e.g. `v5`) |
| `dialect_report` | object | yes | Adapter-specific compatibility report |
| `alias_map_fingerprint` | string | no | SHA-256 over canonical JSON alias input |
| `unresolved_fields` | string[] | yes | Canonical merge-field references still unresolved after alias migration |
| `supported_output_matrix` | object[] | yes | Declared output channels this template may feed when preflight passes |
| `renderer_identity` | object | no | Present when a host renderer binding is declared for the template |
| `definition_fingerprint` | string | no | SHA-256 over canonical template + alias + dialect report |
| `diff_fingerprint` | string | no | SHA-256 over diff against a prior approved definition |
| `approval_handoff` | object | yes | Host-owned readiness gate; core never mutates approval state |
| `authorized_context_fingerprint` | string | no | Binding fingerprint when preflight runs under authorized merge context |

Reports are **payload-free**: no document bytes, merge values, retrieval
results, or credentials.

## `dialect_report` (DOCX V5)

When `dialect` is `v5`, `dialect_report` matches `CompatibilityReport`:

| Field | Type | Notes |
|-------|------|-------|
| `dialect` | string | `v5` |
| `findings[]` | array | Each finding has `category`, `part`, `construct`, optional `reference`, `detail` |
| `migrated` | integer | Count of `migrated` findings |
| `approximated` | integer | Count of `approximated` findings |
| `unsupported` | integer | Count of `unsupported` findings |
| `unsafe_constructs` | integer | Count of `unsafe` findings |
| `unresolved` | integer | Count of `unresolved` findings |

Finding categories:

| Category | Meaning |
|----------|---------|
| `migrated` | Construct maps to a canonical merge field |
| `approximated` | Construct mapped with an explicit approximation |
| `unsupported` | Construct detected but not rendered in the measured floor |
| `unsafe` | Legacy code or unsafe construct; migration blocked |
| `unresolved` | Known construct with missing merge data or alias target |

Executable corpus baselines live under `examples/legacy-corpus/fixtures/<fixture-id>/expected-report.json`.

## `supported_output_matrix`

Each entry declares one output channel the template may produce after successful
rendering or host conversion:

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `channel` | string | yes | One of `docx`, `html_email`, `plain_email`, `memo`, `sms`, `report`, `invoice`, `pdf` |
| `fixture_id` | string | no | Measured corpus or package fixture ID when applicable |
| `status` | string | yes | One of `supported`, `detect_only`, `host_owned`, `deferred` |

Examples:

- `v5-legal-repeat-rendered` → `{ "channel": "docx", "fixture_id": "v5-legal-repeat-rendered", "status": "supported" }`
- `v5-repeat-marker-detected` → `{ "channel": "docx", "fixture_id": "v5-repeat-marker-detected", "status": "detect_only" }`
- Legal PDF → `{ "channel": "pdf", "status": "host_owned" }` (no default local renderer)

## `renderer_identity`

Optional host-supplied renderer metadata for conversion or render receipts:

| Field | Type | Notes |
|-------|------|-------|
| `renderer_id` | string | Stable converter or adapter identity |
| `renderer_version` | string | Version string reported by the host |
| `environment_id` | string | Pinned environment identity (fonts, OS image, etc.) |

The repository conformance corpus may record deterministic fixture metadata; it
does not ship a default PDF converter.

## `approval_handoff`

Host-owned readiness gate. The portable core records handoff metadata only.

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `status` | string | yes | One of `blocked`, `review_required`, `ready_for_host_approval` |
| `blocking_codes` | string[] | yes | Stable codes explaining why production use is blocked |
| `source_definition_fingerprint` | string | no | Prior approved definition fingerprint when diffing |
| `host_approval_state` | string | no | Opaque host workflow state; never interpreted by core |

Typical blocking codes:

| Code | When emitted |
|------|--------------|
| `TQX_UNSUPPORTED_CONSTRUCT` | Unsupported or unsafe construct in `dialect_report` |
| `TQX_UNRESOLVED_FIELD` | Required merge field unresolved after alias migration |
| `TQX_AUTHORIZED_CONTEXT_MISSING` | Required authorized merge context absent |
| `TQX_AUTHORIZED_CONTEXT_STALE` | Context expired or fingerprint mismatch |
| `TQX_RENDERER_UNDECLARED` | PDF or host-owned channel requested without renderer identity |
| `TQX_DIFF_DRIFT` | `diff_fingerprint` differs from approved baseline |

A report with `approval_handoff.status` other than `ready_for_host_approval`
must not be treated as production-ready by hosts or operators.

## Preflight guidance by scenario

| Scenario | Expected `approval_handoff.status` | Operator action |
|----------|----------------------------------|-----------------|
| Alias-only migration succeeds, no unsupported constructs | `review_required` | Review alias map and unresolved fields |
| Unknown alias target | `blocked` | Extend alias map or fix source template |
| Unsupported repeat/conditional marker (detect-only fixture) | `blocked` | Redesign region or defer to escape-hatch work |
| Version drift with matching diff fingerprint | `review_required` | Confirm intentional change |
| Version drift with changed diff fingerprint | `blocked` | Re-approve definition |
| Missing authorized context on a package that requires it | `blocked` | Host must inject `_templiqx_authorized_merge` |
| Optional PDF renderer absent in host | `blocked` for PDF matrix entry only | Wire host converter; DOCX/HTML may still pass |

## Boundaries

- Preflight does not execute legacy template code or spawn host converters.
- Unsupported constructs are report data, not transport errors, unless path
  confinement or archive safety checks fail first.
- Only fixture-backed construct IDs may appear as `supported` in
  `supported_output_matrix`.
- Multi-output receipts remain payload-free; this report never embeds artifact
  bodies.

## Related documents

- [Document inspection v1alpha1](document-inspection-v1alpha1.md)
- [Cross-opco reference packages v1alpha1](cross-opco-reference-packages-v1alpha1.md)
- [Host integration guide](../guides/host-integration.md)
