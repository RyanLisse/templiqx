# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Templiqx is a standalone, provider-neutral AI interaction contract compiler: a portable contract format (`templiqx/v1alpha1`, strict YAML), a canonical application service, local filesystem composition, a CLI, an MCP server, deterministic mock/runtime adapters, and a CRM3 conformance package that ties those pieces together end to end. Rust workspace, edition 2024, `unsafe_code = "forbid"` workspace-wide.

## Commands

```bash
just verify          # fmt --check, clippy -D warnings, workspace tests, boundary checks — run before any PR
just verify-deploy    # docker/kind/supply-chain smoke tests + boundary checks

cargo test --workspace --all-features          # full test suite
cargo test -p templiqx-conformance --test crm3  # one conformance test file
cargo test -p templiqx-local -- service::       # scoped by module/name within a crate

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

./scripts/check-boundaries.sh     # dependency-boundary lints, see below
./scripts/docker-smoke.sh
./scripts/kind-smoke.sh
./scripts/supply-chain-smoke.sh
```

CLI entrypoint: `cargo run -p templiqx-cli -- <command> --root <package-dir> [--json]`. Commands map 1:1 to `TempliqxService`'s capability catalog (`catalog`, `discover`, `inspect`, `put`, `validate`, `compile`, `render`, `execute`, `test`, `diff`, `explain`, `migrate`, ...) — run `cargo run -p templiqx-cli -- --help` for the full, current list rather than trusting a hardcoded copy here. Exit codes are meaningful: `0` = ok envelope, `2` = diagnostic/product-level failure, `1` = CLI/IO failure before the operation ran.

## Architecture

Single canonical application service (`TempliqxService`) with thin transport adapters around it — the CLI and the MCP server are not separate implementations, they call the same operations and get the same envelopes/diagnostics/fingerprints. There is deliberately no separate "agent path."

Dependency direction:

```
contracts <- ports
    ^          ^
    |          |
   core     adapters
     \        /
     application
          |
    local composition
          |
     CLI / MCP / tools
```

- `templiqx-contracts` — serializable DTOs, diagnostics, fingerprints, envelopes. No policy.
- `templiqx-core` — parsing, validation, rendering, compilation. Deterministic, portable.
- `templiqx-ports` — host-facing traits (package storage, artifact workspace, runtime execution, legacy import, document rendering) that a host must implement.
- `templiqx-application` — actor-neutral operations + capability catalog introspection over the ports.
- `templiqx-local` — the only concrete composition today: filesystem-backed storage/workspace + deterministic fake adapters. Enforces path safety (package roots must be canonical dirs under the workspace root, artifact paths are package-relative only, no absolute paths/traversal/backslashes/symlink escapes) and contract writes via compare-and-swap + locking + atomic rename.
- `templiqx-cli`, `templiqx-mcp` — transport surfaces over `templiqx-application`. Tool/command names match the catalog exactly, by design, so an operation can be grepped across CLI/MCP/service code without translation.
- `templiqx-mock`, `adapters/templiqx-runtime-http-mock` — deterministic/conformance-oriented adapters, not production policy engines.
- `adapters/templiqx-docx-v5` — explicit, narrow DOCX V5 compatibility for the CRM3 fixture (body paragraphs, table cells, header/footer, split-run alias migration, MERGEFIELD, repeated/unresolved references). Not general DOCX support — don't generalize it without new fixtures and tests.
- `tools/templiqx-mock-gateway`, `tools/templiqx-http-conformance` — operational tooling for readiness/conformance, not part of the core graph.

### The enforced boundary (`scripts/check-boundaries.sh`)

This is not just convention, it's checked in CI and by `just verify`:

- `templiqx-contracts`, `templiqx-ports`, `templiqx-core` must never depend on a model-provider SDK (openai/anthropic/gemini/bedrock) or on CRM3/rmcp-specific crates. Auth, tenant policy, approval, retries, retrieval, and secrets are host concerns and stay out of the portable core.
- `templiqx-application`, `templiqx-cli`, `templiqx-mcp` (the default composition) must never depend on `templiqx-mock` / `templiqx-runtime-http-mock` / `templiqx-mock-gateway`. Mocks are conformance-only, not reachable from the default runtime path.
- HTTP transport mock crates/implementations must stay out of `templiqx-core`, `templiqx-contracts`, `templiqx-ports`, `templiqx-application`, `templiqx-cli`, `templiqx-mcp` — HTTP mocking is an edge concern.

When touching crate dependencies, `Cargo.toml` per-crate manifests, or adapter wiring, run `./scripts/check-boundaries.sh` — a passing build/clippy does not catch a boundary violation.

### Contract format

`templiqx/v1alpha1` (see `docs/contracts/v1alpha1.md`) is intentionally conservative: unknown fields/enum values are rejected, one contract = one model interaction, inputs/host context are JSON-Schema typed, structured content is data not executable source, content nodes are bounded (`text`, `interpolate`, `when`, `for_each`, `component`), expressions are limited to references/JSON literals/equality/boolean logic/a small filter set. Compilation requires an explicit target capability profile; a contract needing a capability outside that profile fails to compile rather than silently degrading. This conservatism is what keeps the core compiler deterministic and portable — resist relaxing the parser/validator to let unsupported structures through.

### CRM3 conformance package (`examples/crm3`)

Synthetic fixture proving a realistic multi-step workflow without real customer data: discover package → validate package/contracts → BLI-61 date-term extraction → BLI-62 drafting from schema-valid extraction output → migrate legacy DOCX V5 template → render final document → assemble a trace receipt from fingerprints/evidence. The conformance tests check that draft output is grounded in the source fragment (no invented facts) — preserve that evidence-grounding check when touching CRM3 fixtures or scenarios (`examples/crm3/scenarios/**`).

## OpenWiki

<!-- OPENWIKI:START -->

This repository uses OpenWiki for recurring code documentation. Start with `openwiki/quickstart.md`, then follow its links to architecture, workflows, domain concepts, operations, integrations, testing guidance, and source maps.

The scheduled OpenWiki GitHub Actions workflow refreshes the repository wiki. Do not hand-edit generated OpenWiki pages unless explicitly asked; prefer updating source code/docs and letting OpenWiki regenerate.

<!-- OPENWIKI:END -->
