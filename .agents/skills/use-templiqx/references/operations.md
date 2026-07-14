# Operation map

| Intent | MCP tool | CLI command |
| --- | --- | --- |
| Discover packages | `discover_packages` | `discover` |
| Inspect | `inspect_contract` | `inspect` |
| Explain | `explain_contract` | `explain` |
| Put contract | `put_contract` | `put` |
| Validate contract/package | `validate_contract` / `validate_package` | `validate` |
| Compile | `compile_contract` | `compile` |
| Render contract messages without a model call | `render_contract` | `render` |
| Execute | `execute_contract` | `execute` |
| Migrate DOCX | `migrate_legacy` | `migrate` |
| Render DOCX | `render_document` | `render-document` |
| List/run evals | `list_evals` / `run_eval` | `list-evals` / `run-eval` |
| Full package tests | `test_package` | `test` |
| Workspace artifacts | `list_workspace_artifacts` / `read_artifact` | `list-workspace-artifacts` / `read-artifact` |

Use `catalog` for the authoritative live operation catalog. Lifecycle mutations use compare-and-swap fingerprints. Re-read state after `TQX_CAS_CONFLICT`. Package update invalidates signatures; package deletion refuses dependents and untracked content.
