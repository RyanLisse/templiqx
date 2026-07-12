# Testing and verification

This repository relies on layered tests and smoke checks rather than a single monolithic suite. The tests are organized around the same semantic boundary as the architecture: contract logic, local composition, transport surfaces, CRM3 conformance, and deployment readiness.

## Main verification commands

The root `justfile` defines two useful commands:

- `just verify` — format, clippy, workspace tests, and boundary checks.
- `just verify-deploy` — Docker smoke, kind smoke, supply-chain smoke, and boundary checks.

These are the first commands to run when changing anything that affects core logic, adapter boundaries, or deployment assumptions.

## Important test areas

### Core and local service tests

- `crates/templiqx-local/tests/service.rs` validates local composition behavior.
- `crates/templiqx-conformance/tests/crm3.rs` exercises the end-to-end CRM3 trace.
- `crates/templiqx-conformance/tests/crm3_actor_boundary.rs` and `crm3_failures.rs` focus on approval/boundary and failure behavior.
- `crates/templiqx-conformance/tests/http_gateway.rs` covers the HTTP gateway/conformance edge.

These tests are the best signal for whether a change preserved the intended host/core split.

### CLI and MCP workspace tests

- `crates/templiqx-cli/tests/workspace.rs`
- `crates/templiqx-mcp/tests/workspace.rs`

These verify that both surfaces use the same canonical service model and workspace behavior.

### Adapter and readiness checks

- `adapters/templiqx-runtime-http-mock/src/tests.rs`
- `scripts/docker-smoke.sh`
- `scripts/kind-smoke.sh`
- `scripts/supply-chain-smoke.sh`
- `scripts/check-boundaries.sh`

These are especially important when modifying deployment, packaging, or adapter code.

## What the tests protect

The test suite is not just checking happy paths. It protects several repository-specific invariants:

- contract fingerprints remain deterministic;
- package manifests are explicit inventories;
- port boundaries reject unsafe paths and symlink escapes;
- CLI and MCP remain thin transport layers over the same catalog;
- CRM3 evidence remains grounded in the source fragment and does not drift;
- DOCX V5 compatibility stays within the documented subset.

## When changing code

A good rule of thumb:

- contract/core changes should trigger workspace tests and conformance tests;
- CLI/MCP changes should trigger workspace tests for both surfaces;
- filesystem/path changes should trigger local service tests and boundary checks;
- deployment/readiness changes should trigger the smoke scripts in addition to workspace tests.
