# CLI

The CLI calls the same `TempliqxService` methods available to Rust hosts and MCP. It adds no product semantics.

```bash
cargo run -p templiqx-cli -- --root examples/packages catalog
cargo run -p templiqx-cli -- --root examples/packages discover
cargo run -p templiqx-cli -- --root examples/packages update-package demo \
  --version 0.2.0 --expected-fingerprint <manifest-fingerprint>
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

## Agent workflows

The same 26-operation catalog is exposed over MCP with identical envelopes, so
an agent can drive Templiqx entirely through catalog primitives. The MCP server
also emits dynamic onboarding `instructions` at initialize (packages root,
discovered package names, and the canonical tool sequence). Three suggested
flows:

1. **Bootstrap a package.** `create_package <name> --version 0.1.0` →
   `put_contract` to author a contract → `validate_package`. On MCP the
   equivalent tools are `create_package`, `put_contract`, `validate_package`.
2. **Author and inspect a contract.** `put_contract` → `explain_contract`
   (returns the diagnostic graph: defined components, unresolved references, and
   fix hints) → `compile_contract` → `execute_contract`. Use `explain_contract`
   to recover from `TQX_COMPONENT_UNDEFINED` and capability gaps before compiling.
3. **Run an eval and read outputs.** `list_evals <package>` → `run_eval
   <package> <contract> <fixture>` (or `test_package` to run all) →
   `render_document` → `list_workspace_artifacts` → `read_artifact` to inspect
generated output without raw filesystem access.

Package and workspace cleanup is available through `delete-package` and
`delete-workspace-artifact`. Both require `--expected-fingerprint`; stale
fingerprints return a structured `TQX_CAS_CONFLICT` and leave data untouched.
`delete-package` also refuses dependent packages and untracked files, while
`update-package` only changes version/description and clears stale signatures.

The MCP binary accepts the package root as its first argument and the writable
workspace as its second. `TEMPLIQX_PACKAGES_ROOT` and
`TEMPLIQX_WORKSPACE_ROOT` provide the equivalent environment configuration.
Agents can inspect `templiqx://workspace` and use the `bootstrap` and
`run-eval` prompt templates; initialize instructions state both roots and the
workspace confinement rules.

Every operation returns a structured envelope with a stable `operation` name,
`ok` flag, `diagnostics`, and content-addressed `fingerprints`, so an agent can chain
steps and verify determinism by comparing fingerprints across runs.
