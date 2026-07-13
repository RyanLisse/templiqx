# Templiqx OpenWiki quickstart

Templiqx is a standalone, provider-neutral AI interaction contract compiler. The repository packages a portable contract format, a canonical application service, local filesystem composition, an MCP surface, a CLI, deterministic mock/runtime adapters, and a CRM3 conformance package that ties those pieces together.

Start here:

- [Architecture overview](architecture.md) — crate layering, adapter boundaries, and why the repo is organized the way it is.
- [Domain and contract model](domains.md) — the v1alpha1 contract format, CRM3, and DOCX V5 compatibility.
- [Workflows](workflows.md) — CLI, MCP, migration, rendering, and conformance flow.
- [Testing and verification](testing.md) — the test suites and smoke checks that protect the boundary rules.

## What this repo is for

The codebase exists to keep the Templiqx core provider-neutral while letting hosts supply runtime, legacy-import, and document-rendering adapters. The central idea is that humans and agents use the same canonical operations and receive the same validation, diagnostics, fingerprints, and artifact outputs.

The current workspace is a Rust monorepo with supporting docs, examples, Docker/Kubernetes readiness assets, and conformance fixtures. The main package graph is declared in the workspace `Cargo.toml` and is organized into:

- `templiqx-contracts` — stable DTOs, fingerprints, envelopes, and other serializable types.
- `templiqx-ports` — host-facing adapter traits and port errors.
- `templiqx-core` — parsing, validation, rendering, compilation, and deterministic contract logic.
- `templiqx-application` — actor-neutral operations exposed by the service.
- `templiqx-local` — filesystem-backed composition and deterministic fake adapters.
- `templiqx-mock` — mock runtime support.
- `templiqx-cli` — the user-facing command-line entrypoint.
- `templiqx-mcp` — the MCP server surface over the same operations.
- `templiqx-conformance` — CRM3 trace and boundary verification.
- `adapters/templiqx-docx-v5` and `adapters/templiqx-runtime-http-mock` — compatibility and mock runtime adapters.
- `tools/templiqx-mock-gateway` and `tools/templiqx-http-conformance` — operational tooling for the readiness and conformance flows.

## High-level map

```text
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

The docs in this wiki intentionally mirror that shape:

- `architecture.md` covers package layering, host policy boundaries, and deployment assumptions.
- `domains.md` covers the portable contract format, CRM3, and document compatibility fixtures.
- `workflows.md` covers how commands and tools use the same application catalog.
- `testing.md` covers the main validation commands and what each suite protects.

## Documentation maintenance

The repository workflow runs OpenWiki on a daily schedule or by manual dispatch, then opens an update pull request containing the generated `openwiki/` pages and OpenWiki instruction files. Treat the pages here as generated documentation: update source or repository docs for normal changes, and use the workflow to refresh this guide.

## Where to go next

If you are changing core behavior, read the architecture page first. If you are changing contract syntax or examples, read the domain page. If you are changing CLI/MCP behavior, read the workflow page. If you are changing adapter boundaries or package layout, expect the tests and smoke scripts to matter.
