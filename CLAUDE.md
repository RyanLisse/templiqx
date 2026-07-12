# CLAUDE.md

Operational guide for Claude Code and other AI agents in this repository.

## What this is

Templiqx is a standalone, provider-neutral AI interaction contract compiler: portable `templiqx/v1alpha1` contracts (strict YAML), a canonical application service, local filesystem composition, CLI, MCP server, deterministic mock/runtime adapters, and a CRM3 conformance package. Rust workspace, edition 2024, `unsafe_code = "forbid"` workspace-wide. Pre-CRM3 readiness POC for Basenet CRM3 (`BLI-*` Linear issues).

## Quick start

```bash
just verify                              # fmt, clippy, tests, boundaries, CI gates, qlty
just verify-deploy                       # docker/kind/supply-chain smoke + boundaries
just fresh-clone                         # isolated worktree + empty Cargo cache

qlty fmt                                 # format (CI + pre-commit expectation)
qlty check --fix --level=low             # lint fixes before commit

cargo test -p templiqx-conformance --test crm3   # CRM3 end-to-end conformance
./scripts/check-boundaries.sh            # after touching Cargo.toml or adapter wiring
```

Scoped tests: `cargo test -p templiqx-local -- service::`. Full suite: `cargo test --workspace --all-features`.

CLI: `cargo run -p templiqx-cli -- <command> --root <package-dir> [--json]`. Commands map 1:1 to `TempliqxService` capabilities (`catalog`, `discover`, `inspect`, `put`, `validate`, `compile`, `render`, `execute`, `test`, `diff`, `explain`, `migrate`, …). Run `--help` for the current list. Exit codes: `0` = ok envelope, `2` = product/diagnostic failure, `1` = CLI/IO failure.

## Documentation map

| Need | Location |
|------|----------|
| Navigation hub | [`docs/README.md`](docs/README.md) |
| Contract format | [`docs/contracts/v1alpha1.md`](docs/contracts/v1alpha1.md) |
| Architecture / deployment | [`docs/architecture/`](docs/architecture/) |
| Pre-CRM3 readiness | [`docs/guides/pre-crm3-readiness.md`](docs/guides/pre-crm3-readiness.md) |
| Host integration | [`docs/guides/host-integration.md`](docs/guides/host-integration.md) |
| CRM3 scenarios | [`examples/crm3/scenarios/`](examples/crm3/scenarios/) |
| Generated code docs | [`openwiki/quickstart.md`](openwiki/quickstart.md) |

## Crate layout

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

| Crate / path | Role |
|--------------|------|
| `templiqx-contracts` | DTOs, diagnostics, fingerprints, envelopes — no policy |
| `templiqx-core` | Parsing, validation, rendering, compilation — deterministic |
| `templiqx-ports` | Host-facing traits (storage, workspace, runtime, legacy import, document render) |
| `templiqx-application` | Actor-neutral operations + capability catalog |
| `templiqx-local` | Filesystem composition; path safety + CAS contract writes |
| `templiqx-cli`, `templiqx-mcp` | Transport surfaces; tool names match catalog exactly |
| `templiqx-conformance` | CRM3 and failure-semantics tests |
| `templiqx-mock`, `adapters/templiqx-runtime-http-mock` | Conformance adapters only |
| `adapters/templiqx-docx-v5` | Narrow DOCX V5 compat for CRM3 fixture — not general DOCX |
| `tools/templiqx-mock-gateway`, `tools/templiqx-http-conformance` | Operational readiness tooling |

## Enforced boundaries (`scripts/check-boundaries.sh`)

Checked in CI and by `just verify`. Touching dependencies or adapter wiring — run the script explicitly.

- **Portable core** (`templiqx-contracts`, `templiqx-ports`, `templiqx-core`): no model-provider SDKs (openai/anthropic/gemini/bedrock) or CRM3/rmcp-specific crates; no host-owned vocabulary (approval, tenant, retrieval, …).
- **Default composition** (`templiqx-application`, `templiqx-cli`, `templiqx-mcp`): no `templiqx-mock`, `templiqx-runtime-http-mock`, or `templiqx-mock-gateway`.
- **HTTP mocks** stay out of core/contracts/ports/application/CLI/MCP — edge concern only.

## Contract format (summary)

`templiqx/v1alpha1` is intentionally conservative: unknown fields rejected, one contract = one model interaction, JSON-Schema typed inputs, bounded content nodes (`text`, `interpolate`, `when`, `for_each`, `component`), limited expressions. Compilation requires an explicit capability profile — unsupported capabilities fail rather than silently degrade. See [`docs/contracts/v1alpha1.md`](docs/contracts/v1alpha1.md). Do not relax the parser/validator without new fixtures and tests.

## CRM3 conformance (`examples/crm3`)

Synthetic multi-step workflow: discover → validate → BLI-61 extraction → BLI-62 drafting → DOCX V5 migrate → render → trace receipt. Conformance tests assert draft output is grounded in source fragments (no invented facts). Preserve that check when editing fixtures or `examples/crm3/scenarios/**`.

## Deployment

- **Docker:** `Dockerfile`, `deploy/compose.yml`, `./scripts/docker-smoke.sh`
- **Kubernetes:** `charts/templiqx/` (lint with `helm lint charts/templiqx -f charts/templiqx/values-mock.yaml`), `./scripts/kind-smoke.sh`
- **Supply chain:** `./scripts/supply-chain-smoke.sh` (SBOM/digest checks; CI pins Syft/Grype)
- **CI jobs:** boundaries → qlty + rust + docker + helm-kind + supply-chain (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml))
