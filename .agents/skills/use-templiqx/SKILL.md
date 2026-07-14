---
name: use-templiqx
description: Operate the Templiqx application through its MCP tools or CLI to discover packages, inspect or compile contracts, execute interactions, migrate legacy DOCX templates, render documents, and manage workspace artifacts. Use when an agent needs to use Templiqx rather than change Templiqx source code.
---

# Use Templiqx

Prefer the `templiqx` MCP server when its tools are available. Otherwise run the CLI from this repository with `cargo run -q -p templiqx-cli --`. Never invent a second semantic path: MCP and CLI expose the same application operations and structured envelopes.

## Establish context

1. Call `catalog` and `discover_packages` (CLI: `catalog`, `discover`).
2. Read the returned initialize instructions or workspace resource when using MCP.
3. Select the package and contract explicitly. Do not assume `demo` unless the user requests an example.
4. Treat package and workspace paths as portable relative paths. Reject absolute paths, traversal, backslashes, and symlink escapes.

## Choose a workflow

- Inspect or diagnose: `inspect_contract` -> `explain_contract` -> `validate_contract`.
- Render, compile, or execute: `validate_contract` -> `render_contract` for deterministic message preview, or `compile_contract` -> `execute_contract` for a runtime call.
- Migrate and render DOCX: `migrate_legacy` -> inspect the compatibility report -> `render_document` -> `read_artifact`.
- Evaluate: `list_evals` -> `run_eval`, or use `test_package` for the entire package.
- Manage artifacts: `list_workspace_artifacts` -> `read_artifact`; delete only with the current expected fingerprint.

Consult [references/operations.md](references/operations.md) for MCP/CLI name differences and mutation rules.

## Handle results

- Check `ok`; do not infer success from process exit alone.
- Report stable diagnostic codes, help text, and source locations without paraphrasing away critical details.
- Preserve and report returned fingerprints. Compare fingerprints when verifying determinism.
- Treat product-level diagnostic failures as valid envelopes, not transport failures.
- On `TQX_CAS_CONFLICT`, re-read current state and ask before overwriting user work.
- Never pass signing keys, provider secrets, or tenant credentials through Templiqx operations.

## Verify completion

Run the narrowest relevant validation or eval after a mutation. For document output, read the returned artifact through Templiqx rather than assuming a host path. Summarize the operation sequence, envelope status, diagnostic codes, and fingerprints.
