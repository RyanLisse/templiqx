# CLI

The CLI calls the same `TempliqxService` methods available to Rust hosts and MCP. It adds no product semantics.

```bash
cargo run -p templiqx-cli -- --root examples/packages catalog
cargo run -p templiqx-cli -- --root examples/packages discover
cargo run -p templiqx-cli -- --root examples/packages validate demo greeting
cargo run -p templiqx-cli -- --root examples/packages compile demo greeting \
  --values values.json --capability structured_output
cargo run -p templiqx-cli -- --root examples/packages test demo \
  --capability structured_output
cargo run -p templiqx-cli -- --root examples migrate crm3 v5 \
  templates/v5-contract-template.docx \
  --aliases examples/crm3/migrations/v5-aliases.json
jq '.merge_data' examples/crm3/evals/bli-62-output.json >/tmp/crm3-merge-data.json
cargo run -p templiqx-cli -- --root examples render-document crm3 \
  templates/v5-contract-template.templiqx-v5.docx \
  /tmp/crm3-merge-data.json baselines/generated.docx
```

Every successful or product-level failed operation prints an operation envelope. `--json` selects compact JSON; the default is pretty JSON. Exit status is `0` for an `ok` envelope, `2` for a diagnostic failure, and `1` only for CLI/I/O failures before an operation can be invoked.

The default local composition includes the optional safe V5 DOCX adapter for `migrate` and `render-document`. Rust hosts that need a compiler-only dependency graph can call `compose_core`, which returns `TQX_UNSUPPORTED` for those two operations.

The document source, template, and output arguments are always portable paths
relative to the named package beneath `--root`; they are not host filesystem
paths. Absolute paths, traversal, backslashes, and symlink escapes are rejected.
The aliases and merge-data JSON files are CLI inputs read before invoking the
canonical operation. A successful migration result includes both `report` and
the package-relative `canonical_template`, which can be passed directly to
`render-document`.
