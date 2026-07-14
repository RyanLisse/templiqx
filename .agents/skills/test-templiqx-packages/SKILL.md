---
name: test-templiqx-packages
description: Validate Templiqx packages and contracts, discover and run deterministic eval fixtures, compare fingerprints, inspect diagnostics, and verify rendered workspace artifacts. Use when testing a Templiqx package, reviewing a contract change, reproducing a fixture, or checking deterministic behavior through MCP or CLI.
---

# Test Templiqx Packages

Prefer Templiqx MCP tools. Fall back to `cargo run -q -p templiqx-cli -- --root <root>` commands when MCP is unavailable.

## Test sequence

1. Run `discover_packages` and confirm the requested package exists.
2. Run `validate_package`. Stop and report exact diagnostics if it fails.
3. Run `list_evals` and select the requested fixture, or all fixtures when the user asks for package readiness.
4. Run `run_eval` for each selected contract/fixture pair. Use `test_package` only when the full suite is intended.
5. For generated documents, call `list_workspace_artifacts` and `read_artifact` using returned package-relative paths.
6. Repeat a deterministic eval when determinism is the acceptance criterion and compare request, output, and receipt fingerprints.

Read [references/reporting.md](references/reporting.md) for the required evidence format.

## Boundaries

- Do not substitute repository unit tests for application evals.
- Do not run the expensive fresh-clone proof unless explicitly requested; it is a separate weekly/on-demand gate.
- Do not use mock conformance results as proof of a real provider integration.
- Do not delete packages or artifacts during testing.
- Do not expose provider or signing secrets in commands or reports.

## Completion criteria

State which package, contracts, and fixture IDs ran. Report every envelope's `ok` status, stable diagnostic codes, and relevant fingerprints. Distinguish application-level failures from CLI/MCP transport failures.
