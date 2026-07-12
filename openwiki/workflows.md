# Workflows

Templiqx workflows are intentionally thin wrappers over the same canonical application service. The CLI, MCP server, and conformance harness all exercise the same core operations.

## CLI workflow

The CLI lives in `crates/templiqx-cli/src/main.rs` and exposes commands for the canonical capability catalog:

- catalog and discovery;
- contract inspection, put, and validation;
- compile, render, execute, and test;
- diff and explain;
- legacy migration and document rendering;
- CRM3 conformance execution.

The CLI prints an operation envelope for both success and product-level failure. Exit codes are distinct:

- `0` for an `ok` envelope;
- `2` for a diagnostic/product failure;
- `1` for CLI or I/O failures before the operation runs.

Portable path handling matters here: source, template, output, and workspace arguments are package-relative or workspace-relative paths, not arbitrary host paths.

## MCP workflow

`crates/templiqx-mcp/src/lib.rs` defines an MCP server over the same capability catalog. The tool names exactly match the application catalog, which keeps the human and agent surfaces aligned.

That alignment is useful for future agents because it means the same operation names can be searched across the CLI, MCP, and service code without translation.

## Local composition workflow

`crates/templiqx-local/src/lib.rs` composes filesystem-backed package storage, workspace resolution, a deterministic fake runtime, and the document/legacy adapters. This is the default way the repository exercises the system in tests and smoke checks.

It is intentionally local and bounded:

- package roots are filesystem directories under a configured workspace;
- package and artifact paths are checked for traversal and symlink escapes;
- runtime execution is deterministic when used with the mock adapter;
- document rendering and legacy migration are host-owned adapter responsibilities.

## CRM3 conformance workflow

The CRM3 conformance test in `crates/templiqx-conformance/tests/crm3.rs` orchestrates the end-to-end scenario:

1. discover the CRM3 package;
2. validate the package and contracts;
3. run BLI-61 extraction;
4. feed schema-valid extraction output into BLI-62 drafting;
5. migrate the DOCX V5 template;
6. render the final document;
7. assemble a trace receipt from the relevant fingerprints and evidence.

This workflow is where the repository proves that multiple operations can be composed without creating a special agent-only path.

## Readiness and smoke workflows

Recent readiness work added Docker, Helm, and smoke scripts so the repo can validate deployment assumptions outside the unit tests. The important scripts are:

- `scripts/docker-smoke.sh`
- `scripts/kind-smoke.sh`
- `scripts/supply-chain-smoke.sh`
- `scripts/check-boundaries.sh`

The `justfile` exposes `verify` and `verify-deploy` recipes that chain the normal checks.

## When changing workflows

Prefer adding or adjusting tests before changing command plumbing. The high-risk areas are path handling, transport mapping, and any workflow that could diverge between the CLI and MCP surfaces.
