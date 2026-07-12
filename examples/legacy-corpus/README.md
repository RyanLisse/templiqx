# Legacy DOCX corpus (synthetic)

Synthetic fixtures expanding measured compatibility coverage for the
`templiqx-docx-v5` adapter. **This corpus does not claim full V5 support** —
only the explicitly listed constructs are tested.

## Coverage matrix

| Fixture ID | Dialect signal | Expected category | Notes |
|------------|----------------|-------------------|-------|
| `v1-beanshell-detected` | V1 BeanShell | `unsafe` | Detection only; no execution |
| `v2-marker-detected` | V2 `${v2:...}` | `unsupported` | Report-only migration |
| `v5-merge-alias-extra` | V5 merge aliases | `migrated` | Extra `$data.path` alias |
| `v5-nested-table` | V5 nested table | `migrated` | Table cell merge fields |
| `v5-header-footer-edge` | V5 header/footer | `approximated` | Header story merge field |

Each fixture ships:

- `fixtures/<id>/document.xml` — minimal OOXML story fragment used in unit tests
- `fixtures/<id>/expected-report.json` — expected migration report categories

Run corpus tests:

```sh
cargo test -p templiqx-docx-v5 legacy_corpus
```

Production customer templates are out of scope; host teams must add sanitized
fixtures after legal review.
