# Actor-neutral capability map

Templiqx has one canonical application service: `TempliqxService`. Rust hosts call it directly; the CLI and MCP server are thin adapters over the same operation methods and `OperationEnvelope` results. A human or an agent therefore receives the same validation, diagnostics, fingerprints, package artifacts, and compare-and-swap behavior.

The rows below are exactly the operations in `templiqx_application::CAPABILITY_CATALOG` (26 operations):

| Canonical operation | Rust application method | CLI command | MCP tool |
| --- | --- | --- | --- |
| `catalog` | `TempliqxService::catalog` | `templiqx catalog` | `catalog` |
| `discover_packages` | `TempliqxService::discover_packages` | `templiqx discover` | `discover_packages` |
| `create_package` | `TempliqxService::create_package` | `templiqx create <name>` | `create_package` |
| `update_package` | `TempliqxService::update_package` | `templiqx update-package <package>` | `update_package` |
| `delete_package` | `TempliqxService::delete_package` | `templiqx delete-package <package>` | `delete_package` |
| `export_package_identity` | `TempliqxService::export_package_identity` | `templiqx export-package-identity <package>` | `export_package_identity` |
| `sign_package` | `TempliqxService::sign_package` | `templiqx sign-package <package>` | `sign_package` |
| `verify_package_trust` | `TempliqxService::verify_package_trust` | `templiqx verify-package-trust <package>` | `verify_package_trust` |
| `inspect_contract` | `TempliqxService::inspect_contract` | `templiqx inspect <package> <contract>` | `inspect_contract` |
| `put_contract` | `TempliqxService::put_contract` | `templiqx put <package> <contract> <source>` | `put_contract` |
| `delete_contract` | `TempliqxService::delete_contract` | `templiqx delete <package> <contract>` | `delete_contract` |
| `validate_contract` | `TempliqxService::validate_contract` | `templiqx validate <package> <contract>` | `validate_contract` |
| `validate_package` | `TempliqxService::validate_package` | `templiqx validate <package>` | `validate_package` |
| `compile_contract` | `TempliqxService::compile_contract` | `templiqx compile <package> <contract>` | `compile_contract` |
| `render_contract` | `TempliqxService::render_contract` | `templiqx render <package> <contract>` | `render_contract` |
| `execute_contract` | `TempliqxService::execute_contract` | `templiqx execute <package> <contract>` | `execute_contract` |
| `migrate_legacy` | `TempliqxService::migrate_legacy` | `templiqx migrate <package> <dialect> <source>` | `migrate_legacy` |
| `render_document` | `TempliqxService::render_document` | `templiqx render-document <package> <template> <data> <output>` | `render_document` |
| `list_workspace_artifacts` | `TempliqxService::list_workspace_artifacts` | `templiqx list-workspace-artifacts <package>` | `list_workspace_artifacts` |
| `read_artifact` | `TempliqxService::read_artifact` | `templiqx read-artifact <package> <path>` | `read_artifact` |
| `delete_workspace_artifact` | `TempliqxService::delete_workspace_artifact` | `templiqx delete-workspace-artifact <package> <path>` | `delete_workspace_artifact` |
| `test_package` | `TempliqxService::test_package` | `templiqx test <package>` | `test_package` |
| `list_evals` | `TempliqxService::list_evals` | `templiqx list-evals <package>` | `list_evals` |
| `run_eval` | `TempliqxService::run_eval` | `templiqx run-eval <package> <contract> <fixture-id>` | `run_eval` |
| `diff_contract` | `TempliqxService::diff_contract` | `templiqx diff <left-package> <left-contract> <right-package> <right-contract>` | `diff_contract` |
| `explain_contract` | `TempliqxService::explain_contract` | `templiqx explain <package> <contract>` | `explain_contract` |

`templiqx crm3-conformance` runs the synthetic CRM3 smoke workload; it is a CLI-only host integration command, not a catalog operation.

Document `source`, `template`, and `output` values are portable paths relative
to the selected package. The application resolves them beneath that package
root and rejects absolute paths, traversal, backslashes, and symlink escapes
before invoking an adapter. Migration results expose both the compatibility
report and the package-relative canonical template path on every surface.

Lifecycle mutations use compare-and-swap fingerprints. Package updates are
deliberately limited to version and description and invalidate existing
signatures. Package deletion refuses to remove a referenced package or any
untracked content. Workspace deletion compares the current byte fingerprint
and applies the same path and symlink confinement as workspace reads.

## Host policy boundary

Authentication, tenant authorization, secrets, signing, and publication approval remain host-owned concerns. A Blinqx opco can require human approval, agent approval, or automated policy checks around an operation, but must not implement a second semantic path with different Templiqx capabilities or artifacts. Both actors ultimately invoke the same canonical operation and receive the same envelope.
