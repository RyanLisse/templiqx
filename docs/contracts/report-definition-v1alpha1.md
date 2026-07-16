---
title: Report definition v1alpha1
---

A report definition is a versioned, declarative package artifact that binds a
host-owned query reference and merge-data paths to one package-local template.
It supports the frozen-definition path used by Option B and hybrid authoring;
it is not an executable query, workflow, or general-purpose template language.

## Artifact

Report definitions are UTF-8 YAML documents. The v1alpha1 shape has exactly
these required fields:

| Field | Type | Notes |
|-------|------|-------|
| `id` | string | Stable package-local definition identifier |
| `version` | string | Definition version; change it when the frozen definition changes |
| `query_binding` | string | Opaque host reference included in the definition fingerprint; portable core never executes it |
| `field_map` | map of string to string | Definition field name to host merge/query source path |
| `template_ref` | string | Package-relative path to the template artifact |
| `target_format` | string | Renderer target such as `docx`, `html`, `typst`, or `xlsx` |
| `approval` | object | Required review metadata described below; never an executable workflow |

Unknown top-level fields are not part of v1alpha1. All strings must be
non-empty. `field_map` must contain at least one entry, and both its keys and
values are opaque identifiers. Hosts resolve its source paths while assembling
the typed merge context; portable core performs no retrieval or query
execution.

`template_ref` is confined to the package: it must be relative, must not escape
through `..`, and should resolve to a template declared by the package. The
format names the bounded renderer selected by the caller; the definition does
not embed code or renderer configuration.

## Review metadata

The `approval` block has exactly three required string fields:

| Field | Notes |
|-------|-------|
| `status` | Host-defined review state captured with this version |
| `approved_by` | Synthetic or host-owned reviewer identifier |
| `approved_at` | Timestamp string supplied by the host |

This block is **metadata only**. Definition tooling may validate, inspect, diff,
and fingerprint the values, but portable core never starts, advances, or
enforces an approval workflow. Hosts own review policy and authorization.

## Package manifest

A package manifest MAY list definition paths with a `definitions` list. Entries
are package-relative paths, using the same list style as `evals` and
`templates`:

```yaml
definitions:
  - definitions/dunning-letter-v1.yaml
```

## Synthetic example

```yaml
id: dunning-letter
version: 1.0.0
query_binding: host-query://synthetic-basenet/dunning-letter/v1
field_map:
  recipient_name: client.name
  outstanding_total: financials.formatted_total
  claims: claims
template_ref: templates/v5-legal-template.docx
target_format: docx
approval:
  status: approved
  approved_by: synthetic-reviewer
  approved_at: "2026-07-16T00:00:00Z"
```

The checked-in fixture is
`examples/packages/basenet-legal/definitions/dunning-letter-v1.yaml` and contains
synthetic values only.

## Stable fingerprint

Definition identity uses the repository's canonical-JSON SHA-256 approach:
parse the YAML into its semantic JSON value, recursively order object keys,
serialize compact JSON, then hash those bytes with SHA-256. YAML comments,
whitespace, and mapping-key order therefore do not affect the fingerprint;
array order and scalar values do.

The conformance test pins the fixture fingerprint and verifies that an
equivalent YAML document with reordered mappings produces the same value. This
fingerprint identifies the complete definition, including its review metadata;
it does not prove that the referenced host query was executed or authorized.

## Boundaries

- `query_binding` and every `field_map` value remain opaque host references.
- A definition points at a bounded package template; it cannot contain embedded
  query logic, arbitrary code, or provider SDK configuration.
- Review metadata records host state but confers no permission in portable core.
- Rendering and output fingerprints remain separate evidence from the
  definition fingerprint.
