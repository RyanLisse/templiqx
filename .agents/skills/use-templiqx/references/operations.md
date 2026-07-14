# Operation map & concrete syntax

MCP tools and CLI commands are thin facades over the same `TempliqxService`
catalog. Prefer MCP tools when the `templiqx` server is connected; fall back to
the CLI otherwise. Both return the same `OperationEnvelope`.

## Global CLI shape

```sh
cargo run -q -p templiqx-cli -- [--root <packages-dir>] [--json] <command> [args]
```

- `--root <dir>` — directory holding portable package directories. **Global**, defaults to `.`. Put it before or after the subcommand.
- `--json` — emit compact JSON (default is pretty JSON). **Global**. Use it whenever you parse output programmatically.
- Exit codes: `0` = `ok` envelope · `2` = product/diagnostic failure (still a valid envelope) · `1` = CLI/IO failure before the operation ran. **Never infer success from exit code alone — read the envelope `ok` field.**

Positional args have no `--`; everything shown with `--` is a named flag. Clap
kebab-cases names, so the `expected_fingerprint` field is `--expected-fingerprint`.

## Full operation table

| Intent | MCP tool | CLI |
| --- | --- | --- |
| Live catalog | `catalog` | `catalog` |
| Discover packages | `discover_packages` | `discover` |
| Create package | `create_package` | `create <name> [--version 0.1.0]` |
| Update package | `update_package` | `update-package <package> --expected-fingerprint <fp> [--version <v>] [--description <d>]` |
| Delete package | `delete_package` | `delete-package <package> --expected-fingerprint <fp>` |
| Export identity | `export_package_identity` | `export-package-identity <package>` |
| Sign package | `sign_package` | `sign-package <package> --key-id <id> --expected-fingerprint <fp>` |
| Verify trust | `verify_package_trust` | `verify-package-trust <package> [--strict]` |
| Inspect contract | `inspect_contract` | `inspect <package> <contract>` |
| Put contract | `put_contract` | `put <package> <contract> <source-file> [--expected-fingerprint <fp>]` |
| Delete contract | `delete_contract` | `delete <package> <contract> --expected-fingerprint <fp>` |
| Validate | `validate_contract` / `validate_package` | `validate <package> [contract]` |
| Compile | `compile_contract` | `compile <package> <contract> [--values <file>] [--capability <cap>]…` |
| Render (no model call) | `render_contract` | `render <package> <contract> [--values <file>] [--capability <cap>]…` |
| Execute (model call) | `execute_contract` | `execute <package> <contract> --fixture-output <file> [--values <f>] [--capability <cap>]… [--stream]` |
| Test whole package | `test_package` | `test <package> [--capability <cap>]…` |
| Diff contracts | `diff_contract` | `diff <left-pkg> <left-contract> <right-pkg> <right-contract>` |
| Explain | `explain_contract` | `explain <package> <contract>` |
| Migrate legacy | `migrate_legacy` | `migrate <package> <dialect> <source> [--aliases <file>]` |
| Render document | `render_document` | `render-document <package> <template> <data-file> <output> [--workspace <dir>]` |
| List artifacts | `list_workspace_artifacts` | `list-workspace-artifacts <package> [--workspace <dir>] [--prefix <p>]` |
| Read artifact | `read_artifact` | `read-artifact <package> <path> [--workspace <dir>]` |
| Delete artifact | `delete_workspace_artifact` | `delete-workspace-artifact <package> <path> --expected-fingerprint <fp> [--workspace <dir>]` |
| List evals | `list_evals` | `list-evals <package>` |
| Run eval | `run_eval` | `run-eval <package> <contract> <fixture-id> [--capability <cap>]…` |

`catalog` is authoritative for the live set — read it first rather than trusting this table.

## Non-obvious flags (the "smart use" details)

- **`execute` requires `--fixture-output <file>`** — the deterministic receipt is written there. Omitting it is a CLI error, not a no-op.
- **Every lifecycle mutation is compare-and-swap**: `update-package`, `delete-package`, `delete`, `sign-package`, `delete-workspace-artifact`, and `put` (when overwriting) take `--expected-fingerprint`. Read the current fingerprint first, pass it, and on `TQX_CAS_CONFLICT` re-read and ask before retrying.
- **Capability profiles are repeatable**: pass `--capability` once per required capability (`--capability text --capability json_output`). Compilation *fails closed* if the contract needs a capability the profile doesn't grant — never work around it by dropping the requirement.
- **`--values <file>`** supplies typed inputs for compile/render/execute; without it the contract's declared inputs must have defaults.
- `render` previews the compiled messages with **no** runtime/model call; `execute` performs the call. Use `render` to check determinism cheaply.
- Package update invalidates existing signatures; package deletion refuses packages with dependents or untracked content.

## Worked recipe — inspect → validate → render (CLI)

```sh
R="--root examples/packages"
cargo run -q -p templiqx-cli -- $R --json catalog                 # discover ops
cargo run -q -p templiqx-cli -- $R --json discover                # list packages
cargo run -q -p templiqx-cli -- $R inspect demo greeting          # structure + refs
cargo run -q -p templiqx-cli -- $R validate demo greeting         # ok envelope?
cargo run -q -p templiqx-cli -- $R render demo greeting \
    --capability text                                             # message preview, no model call
```

## Worked recipe — same flow over MCP

```jsonc
// tool: catalog        args: {}
// tool: discover_packages  args: {}
// tool: inspect_contract   args: { "package": "demo", "contract": "greeting" }
// tool: validate_contract  args: { "package": "demo", "contract": "greeting" }
// tool: render_contract    args: { "package": "demo", "contract": "greeting", "capabilities": ["text"] }
```

MCP arguments use the field names shown in `catalog`; the `--capability` repeatable
flag maps to a `capabilities` array. Connect the server per `docs/guides/agent-native.md`
in the repository: the stdio command is
`cargo run -p templiqx-mcp -- <packages-root> [workspace-root]`.
