# OpenWiki init plan

## Pages to create
1. `openwiki/quickstart.md`
   - Evidence: `/Cargo.toml`, `/docs/README.md`, `/docs/architecture/poc.md`, `/docs/architecture/capability-map.md`, `/docs/architecture/deployment.md`, `/docs/contracts/v1alpha1.md`, `/docs/contracts/mock-scenarios-v1alpha1.md`, `/docs/guides/cli.md`, `/docs/guides/pre-crm3-readiness.md`, `/examples/crm3/README.md`
   - Purpose: repository overview, architecture map, major workflows, and navigation entrypoint.
2. `openwiki/architecture.md`
   - Evidence: `/docs/architecture/poc.md`, `/docs/architecture/capability-map.md`, `/docs/architecture/deployment.md`, `/Cargo.toml`, `/crates/templiqx-application/src/lib.rs`, `/crates/templiqx-ports/src/lib.rs`, `/crates/templiqx-local/src/lib.rs`, `/crates/templiqx-mcp/src/lib.rs`
   - Purpose: explain crate layering, the canonical service, port boundaries, host policy, and deployment/runtime split.
3. `openwiki/domains.md`
   - Evidence: `/docs/contracts/v1alpha1.md`, `/docs/contracts/mock-scenarios-v1alpha1.md`, `/examples/crm3/README.md`, `/adapters/templiqx-docx-v5/README.md`, `/adapters/templiqx-runtime-http-mock/README.md`, `/docs/guides/pre-crm3-readiness.md`
   - Purpose: explain contract format, CRM3, DOCX V5 compatibility, mock runtime, and business/product concepts.
4. `openwiki/workflows.md`
   - Evidence: `/docs/guides/cli.md`, `/docs/guides/pre-crm3-readiness.md`, `/crates/templiqx-cli/tests/workspace.rs`, `/crates/templiqx-mcp/tests/workspace.rs`, `/crates/templiqx-conformance/tests/http_gateway.rs`, `/crates/templiqx-conformance/tests/crm3_actor_boundary.rs`
   - Purpose: describe CLI/MCP usage, workspace semantics, conformance flow, and approval/retry boundaries.
5. `openwiki/testing.md`
   - Evidence: same test files plus `/crates/templiqx-local/tests/service.rs`, `/crates/templiqx-conformance/tests/crm3_failures.rs`, `/crates/templiqx-conformance/tests/crm3.rs`
   - Purpose: explain how to validate changes, where the key tests live, and what behaviors they protect.

## Remaining questions
- Whether docs should also include a source-map page for crate/package ownership or keep that embedded in architecture/workflows pages.
- Whether the tools crates (`tools/templiqx-mock-gateway`, `tools/templiqx-http-conformance`) need dedicated wiki pages or can be covered in workflows/testing.
- Whether the repo uses any non-Rust operational flows beyond `just verify` / `just verify-deploy` that should be highlighted.