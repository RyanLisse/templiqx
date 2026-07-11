# POC architecture

Templiqx is a standalone, provider-neutral AI interaction contract compiler. Its dependency direction is:

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
         CLI / MCP / Rust hosts
```

`templiqx-contracts` contains stable serializable DTOs. `templiqx-core` parses, validates, renders and compiles without network access. `templiqx-ports` defines package, runtime, legacy-import and document-rendering seams. `templiqx-application` exposes one actor-neutral catalog of atomic operations. Every transport is a thin facade over that service: humans and agents use the same capabilities, package artifacts, diagnostics and compare-and-swap rules. A host may apply identity, authorization or publication approval policy around the service, but Templiqx does not create a more privileged agent-only or human-only path.

The core package graph deliberately contains no CRM3/Basenet domain dependency, model-provider SDK, credentials, authorization policy, workflow engine, RAG implementation, or legacy executable runtime. A host owns those concerns and supplies adapters.

## Package layout

```text
<root>/<package>/
  templiqx.yaml       # strict manifest and explicit inventory
  contracts/*.yaml
  components/
  evals/
  migrations/
  templates/
```

The manifest is an explicit inventory. Validation reads every listed contract, component, eval, migration, template and baseline artifact; a missing artifact invalidates the package. Duplicate inventory paths are rejected, including the same path listed in different sections.

The filesystem store permits only safe single-segment package and contract identifiers and canonical relative artifact paths. Absolute paths, `.`/`..`, empty path segments, backslash aliases, traversal, symlinked package directories, and symlinks at any artifact path segment are rejected. Roots are canonicalized, and successful reads must remain beneath the package root and resolve to regular files. Contract writes use compare-and-swap under an advisory package lock, temporary files, and atomic rename.

The local POC assumes the configured package root is owned by the invoking
host and is not concurrently mutated by an untrusted local process. Its checks
reject static traversal and symlink escapes, but the ordinary `PathBuf` adapter
ports do not claim descriptor-anchored protection against a hostile check/use
race. A production multi-user host must keep the root behind its authorization
boundary or supply an `openat2`/`cap-std` style store. This limitation does not
create separate human and agent capabilities.

## Identity and determinism

Contract identity is semantic: DTOs are converted to JSON, object keys are recursively ordered, serialized without whitespace, and hashed with SHA-256. Source formatting and YAML map order therefore do not affect the individual contract fingerprint.

Package identity is stricter. Manifest inventory lists are normalized into sorted order, every manifest-listed artifact is read as exact bytes, and the sorted path-to-byte-SHA-256 map is hashed together with the normalized manifest. Consequently, changing any byte—including source-only whitespace in a contract, a DOCX member, an eval, a migration, or a baseline—changes package identity. Merely reordering an otherwise identical manifest inventory does not. An invalid or incomplete inventory receives no package fingerprint.

The same valid contract, inputs, context, target capability profile and compiler version yield the same compiled interaction.

## Adapter boundary

The shipped fake runtime accepts an explicit fixture output, validates it against the contract's JSON Schema, and emits payload fingerprints plus its adapter identity. It never calls a network. Production gateway adapters remain host-owned.

## Conformance trace boundary

The CRM3 pilot composes two atomic interactions and a document migration in
the host-neutral `templiqx-conformance` harness. The public,
payload-free `ConformanceTraceReceipt` DTO joins the package, interaction,
adapter, schema, evaluation, migration, render and baseline fingerprints, and
the CRM3 integration test proves its construction. Receipt composition is
deliberately not a fourteenth application operation: it spans multiple model
interactions and therefore remains at the host/orchestration boundary. Humans
and agents exercise the same underlying Rust, CLI and MCP envelopes from which
the same receipt is composed.
