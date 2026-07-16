---
title: Merge data v1alpha1
---

Portable merge data is a JSON object supplied to a bounded document adapter.
It contains only values that are ready for deterministic rendering: adapters do
not fetch records, follow links, or call provider APIs while resolving fields.

## Merge-data object

The top-level object may contain package-specific namespaces such as `client`,
`parties`, or `financials`. Portable packages reserve `customFields` for
host-defined fields that do not belong in a shared domain namespace.

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `customFields` | object | no | Flat map of host-defined field keys to typed render values |
| other fields | any JSON value | package-defined | Declared by the package contract and template |

Keys are case-sensitive. A package must use the same canonical path in its
contract output, merge-data fixture, and document template.

## `customFields` namespace

`customFields` is a flat map of `string -> value`. Nested grouping below the
field key is not portable. Every value uses one of these shapes:

| `type` | Shape | Renderable leaf |
|--------|-------|-----------------|
| `text` | `{ "type": "text", "value": "..." }` | `customFields.<key>.value` |
| `relation_link` | `{ "type": "relation_link", "display": "...", "ref": "..." }` | `customFields.<key>.display` |

For `relation_link`, `display` is the pre-resolved display name and `ref` is an
opaque host reference. The host resolves the display name before invoking
Templiqx. The renderer never dereferences `ref`, which keeps the portable
template engine deterministic and IO-free as required by BLI-65.

```json
{
  "client": { "name": "Voorbeeld A B.V." },
  "customFields": {
    "rechtsgebied": {
      "type": "text",
      "value": "Handelsrecht"
    },
    "behandelend_advocaat": {
      "type": "relation_link",
      "display": "mr. Eva de Vries",
      "ref": "SYN-REL-LAWYER-0001"
    }
  }
}
```

## Resolution and compatibility preflight

Template paths address the renderable leaf, for example
`${customFields.rechtsgebied.value}` or
`${customFields.behandelend_advocaat.display}`. The payload-free inspection
phase inventories these references but cannot decide whether they are present.
When the template is checked against supplied merge data, the bounded adapter
reports any missing `customFields.*` path. Compatibility assembly copies the
full path to `unresolved_fields` and emits the ordinary unresolved-field
diagnostic. Missing fields never trigger a host lookup or silently render a
related record.

## Boundaries

- Merge data contains render-ready values, not query instructions or provider
  SDK objects.
- `relation_link.ref` is preserved as opaque metadata and is not rendered or
  dereferenced unless a package explicitly targets that scalar path.
- Package contracts may enumerate a bounded fixture-specific subset of custom
  field keys even though the portable namespace permits different flat keys in
  other packages.
- Unknown `customFields.*` paths follow the same unresolved-field behavior as
  every other unknown merge path.

## Related documents

- [Template compatibility report v1alpha1](template-compatibility-report-v1alpha1.md)
- [Cross-opco reference packages v1alpha1](cross-opco-reference-packages-v1alpha1.md)
- [Templiqx contract v1alpha1](v1alpha1.md)
