<p align="center">
  <img src="docs/specs/pre-crm3-readiness-docker-kubernetes/hero.png" alt="Templiqx: verified, deterministic, deployed" width="720">
</p>

<h1 align="center">Templiqx</h1>

<p align="center">
  A standalone, provider-neutral AI interaction contract compiler.
</p>

<p align="center">
  <a href="https://github.com/RyanLisse/templiqx/actions/workflows/ci.yml"><img src="https://github.com/RyanLisse/templiqx/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/edition-2024-blue" alt="Rust edition 2024">
  <img src="https://img.shields.io/badge/unsafe__code-forbid-critical" alt="unsafe_code forbid">
  <img src="https://img.shields.io/badge/license-Apache--2.0-informational" alt="Apache-2.0">
</p>

Templiqx turns portable `templiqx/v1alpha1` contracts (strict YAML) into deterministic, fingerprinted AI interactions — one contract, one model interaction, no invented facts. A single actor-neutral service (`TempliqxService`) backs the CLI, an MCP server, and any Rust host, so humans and agents get identical validation, diagnostics, and compare-and-swap package writes. It ships as the current pre-CRM3 readiness proof-of-concept for Basenet CRM3 (`BLI-*`).

## Why it's structured this way

- **Portable core, host-owned edges.** `templiqx-contracts` / `templiqx-ports` / `templiqx-core` never import a model-provider SDK, CRM3 vocabulary, or credentials — those live in adapters the host wires in explicitly. `scripts/check-boundaries.sh` enforces this in CI and in `just verify`.
- **One capability catalog, every transport.** CLI commands and MCP tools are thin facades over the same `templiqx_application::CAPABILITY_CATALOG` methods — no agent-only or human-only path.
- **Determinism is provable, not assumed.** Contract identity is a SHA-256 over canonically-ordered JSON; package identity hashes every manifest-listed artifact's exact bytes. Same contract + inputs + capability profile + compiler version → same compiled interaction, every time.

## Quick start

```bash
just verify                              # fmt, clippy, tests, boundary checks — run before any PR
just verify-deploy                       # docker/kind/supply-chain smoke + boundaries

qlty fmt                                 # format (CI + pre-commit expectation)
qlty check --fix --level=low             # lint fixes before commit

cargo test -p templiqx-conformance --test crm3   # CRM3 end-to-end conformance
./scripts/check-boundaries.sh            # after touching Cargo.toml or adapter wiring
```

CLI usage: `cargo run -p templiqx-cli -- <command> --root <package-dir> [--json]`. Commands map 1:1 to `TempliqxService` capabilities — run `--help` for the current list. Exit codes: `0` = ok envelope, `2` = product/diagnostic failure, `1` = CLI/IO failure.

## Architecture

```mermaid
flowchart TD
    Contracts["templiqx-contracts\nDTOs, diagnostics, fingerprints"]
    Ports["templiqx-ports\nhost-facing traits"]
    Core["templiqx-core\nparse / validate / render / compile"]
    Adapters["adapters/*\nhost-owned (mock, http-mock, docx-v5, langfuse)"]
    Application["templiqx-application\nactor-neutral capability catalog"]
    Local["templiqx-local\nfilesystem composition + CAS writes"]
    CLI["templiqx-cli"]
    MCP["templiqx-mcp"]

    Contracts --> Core
    Ports --> Core
    Ports --> Adapters
    Core --> Application
    Adapters --> Application
    Application --> Local
    Local --> CLI
    Local --> MCP
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
| `adapters/templiqx-runtime-langfuse` | Host-owned production `RuntimeAdapter`: real chat completion + best-effort Langfuse tracing |
| `tools/templiqx-mock-gateway`, `tools/templiqx-http-conformance` | Operational readiness tooling |

## Capability catalog

Every row is a canonical `TempliqxService` operation, exposed identically over the CLI and MCP:

| Operation | CLI | MCP tool |
|-----------|-----|----------|
| `discover_packages` | `templiqx discover` | `discover_packages` |
| `inspect_contract` | `templiqx inspect <package> <contract>` | `inspect_contract` |
| `put_contract` | `templiqx put <package> <contract> <source>` | `put_contract` |
| `validate_contract` | `templiqx validate <package> <contract>` | `validate_contract` |
| `validate_package` | `templiqx validate <package>` | `validate_package` |
| `compile_contract` | `templiqx compile <package> <contract>` | `compile_contract` |
| `render_contract` | `templiqx render <package> <contract>` | `render_contract` |
| `execute_contract` | `templiqx execute <package> <contract>` | `execute_contract` |
| `migrate_legacy` | `templiqx migrate <package> <dialect> <source>` | `migrate_legacy` |

A host may wrap approval, authorization, or automated policy around any of these — it must not implement a second semantic path to the same artifacts.

## CRM3 conformance

`examples/crm3` is a standalone, synthetic package proving the BLI-61 → BLI-62 interaction boundary plus explicit DOCX V5 compatibility. It imports no Basenet code and contains no customer data. BLI-62's draft is grounded in BLI-61's schema-validated extraction — the conformance test fails if a fact isn't traceable back to a source fragment.

```mermaid
sequenceDiagram
    participant Host
    participant Service as TempliqxService
    participant Runtime as RuntimeAdapter
    participant Docx as DOCX V5 adapter

    Host->>Service: validate_package(crm3)
    Service-->>Host: OperationEnvelope(ok)
    Host->>Service: execute_contract(BLI-61 extraction)
    Service->>Runtime: execute(request)
    Runtime-->>Service: ExecutionReceipt (grounded facts)
    Service-->>Host: schema-validated extraction
    Host->>Service: execute_contract(BLI-62 drafting)
    Service->>Runtime: execute(request)
    Runtime-->>Service: ExecutionReceipt (draft)
    Service-->>Host: schema-validated draft
    Host->>Service: migrate_legacy(V5 template)
    Service->>Docx: migrate + render
    Docx-->>Service: rendered baseline
    Service-->>Host: ConformanceTraceReceipt (fingerprints only)
```

Run it: `cargo test -p templiqx-conformance --test crm3`.

## Deployment

```mermaid
flowchart LR
    subgraph CI["CI (.github/workflows/ci.yml)"]
        direction LR
        Boundaries["boundaries"] --> Qlty["qlty"] --> Rust["rust tests"] --> Docker["docker smoke"] --> Helm["helm-kind smoke"] --> Supply["supply-chain"]
    end
    Docker --> Image["Container image"]
    Helm --> Chart["charts/templiqx (Helm)"]
    Supply --> SBOM["SBOM + digest (Syft/Grype)"]
```

- **Docker:** `Dockerfile`, `deploy/compose.yml`, `./scripts/docker-smoke.sh`
- **Kubernetes:** `charts/templiqx/` (lint with `helm lint charts/templiqx -f charts/templiqx/values-mock.yaml`), `./scripts/kind-smoke.sh`
- **Supply chain:** `./scripts/supply-chain-smoke.sh` — SBOM/digest checks; CI pins Syft/Grype

## Enforced boundaries

Checked in CI and by `just verify` — run `./scripts/check-boundaries.sh` explicitly after touching `Cargo.toml` or adapter wiring:

- **Portable core** (`templiqx-contracts`, `templiqx-ports`, `templiqx-core`): no model-provider SDKs, no CRM3/rmcp-specific crates, no host-owned vocabulary (approval, tenant, retrieval, …).
- **Default composition** (`templiqx-application`, `templiqx-cli`, `templiqx-mcp`): no `templiqx-mock`, `templiqx-runtime-http-mock`, or `templiqx-mock-gateway`.
- **HTTP mocks** stay out of core/contracts/ports/application/CLI/MCP — edge concern only.

## Documentation

| Need | Location |
|------|----------|
| Navigation hub | [`docs/README.md`](docs/README.md) |
| Contract format | [`docs/contracts/v1alpha1.md`](docs/contracts/v1alpha1.md) |
| Architecture / deployment detail | [`docs/architecture/`](docs/architecture/) |
| Pre-CRM3 readiness | [`docs/guides/pre-crm3-readiness.md`](docs/guides/pre-crm3-readiness.md) |
| CRM3 scenarios | [`examples/crm3/scenarios/`](examples/crm3/scenarios/) |
| Generated code docs | [`openwiki/quickstart.md`](openwiki/quickstart.md) |
| Agent operating guide | [`CLAUDE.md`](CLAUDE.md) |
