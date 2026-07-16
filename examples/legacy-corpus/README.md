# Legacy DOCX corpus (synthetic)

Synthetic fixtures expanding measured compatibility coverage for the
`templiqx-docx-v5` adapter. **This corpus does not claim full V5 support** —
only the explicitly listed constructs are tested.

## Coverage matrix

| Fixture ID | Dialect signal | Expected category | Notes |
|------------|----------------|-------------------|-------|
| `v1-beanshell-detected` | V1 BeanShell | `unsafe` | Detection only; no execution |
| `v2-marker-detected` | V2 `${v2:...}` | `unsupported` | Report-only migration |
| `v5-repeat-marker-detected` | V5 `${#...}` / `${/...}` | `unsupported` | Repeat markers detected; not rendered in this slice |
| `v5-conditional-marker-detected` | V5 `${?...}` / `${/...}` | `unsupported` | Conditional regions detected; not rendered in this slice |
| `v5-legal-repeat-rendered` | V5 bounded table-row repeat | `migrated` | Three repeated claim rows; normalized OOXML parity |
| `v5-legal-conditional-rendered` | V5 bounded paragraph conditional | `migrated` | Optional clause included/excluded by truthy merge data |
| `v5-nested-table` | V5 nested table | `migrated` | Placeholder plus merge field in a nested table |
| `v5-header-footer` | V5 story parts | `migrated` | Body, header, and footer rendering |
| `v5-alias-collision-missing` | V5 aliases | `migrated` | Two aliases converge; missing merge data stays unresolved |
| `invalid-corrupt` | Invalid ZIP | rejected | Non-ZIP input fails closed |
| `invalid-oversized-entry` | ZIP limit | rejected | Per-entry expansion limit is enforced |
| `invalid-traversal` | Unsafe member path | rejected | `../` ZIP member fails closed |

Supported/detected fixtures ship:

- `fixtures/<id>/source.docx` — a real minimal OOXML package
- `fixtures/<id>/aliases.json` — deterministic migration input
- `fixtures/<id>/expected-report.json` — the exact compatibility report
- `fixtures/<id>/render-data.json` and `expected-render.docx` where rendering is supported

Hostile fixtures ship `expected-error.json` and are asserted to fail closed.
All ZIP member order, timestamps, permissions, XML, and JSON formatting are
generated deterministically by `tools/templiqx-legacy-docx-fixtures`.

Run corpus tests:

```sh
cargo run -p templiqx-legacy-docx-fixtures
cargo test -p templiqx-legacy-docx-fixtures
cargo test -p templiqx-docx-v5 legacy_corpus
cargo test -p templiqx-conformance --test legal_docx
cargo test -p templiqx-conformance --test reference_package_claims
```

## Real v5 report templates (reference, not fixtures)

[`v5-report-templates/`](v5-report-templates/) holds three **real** legacy
Basenet v5 report templates (company overview, case cover, WIP per cost centre)
as format/dialect evidence for the report engine. They are reference-only:
outside `fixtures/`, so the conformance scanners do not load them, and they
carry **no client data** — only merge-field/Velocity template logic. See
[`v5-report-templates/README.md`](v5-report-templates/README.md) and
[`docs/guides/report-engine-compatibility.md`](../../docs/guides/report-engine-compatibility.md).

Production *customer* templates (filled with client data) remain out of scope;
host teams must add sanitized fixtures after legal review.
