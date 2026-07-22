---
title: Testing and verification
---

This repository uses a layered verification model: fast unit and workspace tests, boundary enforcement, conformance tests, and deployment/supply-chain smoke checks.

## Main commands

- `just verify` — the normal local gate for formatting, linting, tests, and boundary checks.
- `just verify-deploy` — adds Docker, kind, and supply-chain smoke checks.
- `just verify-all` — the broad local gate that includes docs and deployment validation.
- `cargo test -p templiqx-conformance --test crm3` — the CRM3 end-to-end proof.
- `./scripts/check-boundaries.sh` — dependency and composition guardrail.

## What each layer protects

### Workspace tests

The Rust workspace tests cover service behavior, local composition, CLI behavior, MCP routing, and adapter compatibility. Good examples to inspect when changing behavior are:

- `crates/templiqx-application/tests/*.rs`
- `crates/templiqx-local/tests/*.rs`
- `crates/templiqx-cli/tests/*.rs`
- `crates/templiqx-conformance/tests/*.rs`

These tests are the fastest way to detect semantic drift. Alongside `crm3.rs`, the conformance crate carries per-format render suites (`html_render.rs`, `markdown_render.rs`, `rtf_render.rs`, `typst_render.rs`, `xlsx_render.rs`, `pdf_render.rs`), plus `streaming.rs`, `document_inspection.rs`, `http_gateway.rs`, and the `cross_opco_*` breadth checks — each pinned to its adapter under `adapters/`.

### Boundary enforcement

`scripts/check-boundaries.sh` is the repo-specific policy gate. It ensures that portable crates stay free of provider SDKs and host vocabulary, and that default composition does not pull in mock runtime or gateway packages.

If you change Cargo dependencies, adapter wiring, or product image composition, run this script directly instead of relying only on `cargo test`.

### CRM3 conformance

`crates/templiqx-conformance/tests/crm3.rs` is the highest-signal product test. It asserts that the CRM3 package can be discovered, validated, executed, migrated, and rendered as a grounded workflow. It also checks evidence traceability so the draft cannot invent facts that are not sourced from the inputs.

### Report-engine benches

The report-engine work added a small bench harness in `tools/templiqx-bench` with two focused entrypoints:

- `report-determinism` — exercises frozen `basenet-legal` DOCX renders repeatedly and checks for stable output hashes;
- `report-fanout` — renders the same migrated template across 1,000 records and checks for corrupt output.

The shared library code lives in `tools/templiqx-bench/src/report_determinism.rs` and `tools/templiqx-bench/src/report_fanout.rs`. When changing report adapters or template handling, use these benches to confirm the render path remains deterministic and can fan out without corruption.

### Deployment and release smoke

The repository includes smoke checks for Docker, kind, and supply chain validation:

- `scripts/docker-smoke.sh`
- `scripts/kind-smoke.sh`
- `scripts/supply-chain-smoke.sh`

These matter when changing image composition, Helm charts, deployment manifests, or release scripts.

## Change guidance

When editing behavior, start with the smallest focused test that exercises the changed path, then expand to the broader gate:

1. add or update unit/workspace tests;
2. run the relevant package tests;
3. run `./scripts/check-boundaries.sh` if dependencies or adapters changed;
4. run the conformance and smoke checks relevant to the area you touched.
