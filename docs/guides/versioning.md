---
title: Engine and SDK versioning
---

Templiqx keeps the engine and all three host SDK packages on one coordinated
semantic-version line. The compatibility matrix is the source of truth:
`engineVersion` and every SDK `sdkVersion` have the same `major.minor.patch`,
while `engineApiVersion` is the matching `major.minor` line.

## Semantic-version policy

- **Patch** — the Operations OpenAPI digest is unchanged and only engine or
  compatibility metadata advances.
- **Minor** — an operation ID is added, or the OpenAPI digest changes without
  removing an operation ID.
- **Breaking** — an operation ID is removed or renamed. The bump tool refuses
  this change rather than modifying the existing Operations API in place.

Classification always compares the working-tree
`openapi/templiqx-operations-v1.yaml` with the version committed at Git `HEAD`.
The matrix records the resulting digest and versions; it does not duplicate an
operation inventory.

## Breaking Operations APIs

The `/operations/vN` path is a wire-compatibility boundary. A breaking HTTP
change requires a new base path such as `/operations/v2`, a corresponding new
OpenAPI document, and a major release. Do not remove or rename operations under
an existing versioned base path.

## Coordinated bump workflow

Preview the deterministic plan from the repository root:

```bash
just bump-engine
# or, explicitly:
just bump-engine --dry-run
```

The safe default is dry: without `--yes`, the command prints its
classification, proposed version, digest delta, operation-ID delta, and every
version-controlled file it would touch, then writes nothing. Override the
proposed version with `--to major.minor.patch` when needed.

Apply the reviewed plan explicitly:

```bash
just bump-engine --yes
just bump-engine --to 0.2.0 --yes
```

The tool writes the compatibility matrix first, aligns the Cargo workspace and
the TypeScript, Python, and .NET package versions, refreshes their lockfiles,
then runs each SDK's existing generator so compatibility markers remain derived
from the matrix. Finally it prepends a changelog stanza describing the engine
and contract delta. It never commits, publishes, or edits hand-written
transport clients. The bump script itself performs no network requests; the
existing .NET generator can populate its pinned generator cache when absent.

Review the resulting ordinary Git diff, then run:

```bash
just bump-check
just compat-check
just verify
```

`bump-check` is the fast tripwire: it fails when the normative OpenAPI digest
does not match the matrix and prints the exact bump command to run.
