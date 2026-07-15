---
title: SDK compatibility matrix
---

[`openapi/compatibility-matrix.yaml`](../../openapi/compatibility-matrix.yaml) is
the single machine-readable compatibility record for the Operations API. It
binds an Operations API version and OpenAPI digest to the contract format,
engine API line, exact current engine version, supported engine range, and
published version of each host SDK.

SDK generators read the matrix and emit checked-in compatibility markers next
to their generated DTOs. SDK compatibility modules consume those markers; they
do not declare engine or contract versions independently.

Use the [versioning and coordinated bump workflow](versioning.md) whenever the
engine version or normative Operations OpenAPI document changes.

## Drift enforcement

Run the compatibility gate from the repository root:

```bash
just compat-check
```

The gate recomputes the SHA-256 digest of the normative OpenAPI document and
compares the matrix with the OpenAPI version and contract-format constant, each
SDK package manifest, all generated markers, and each compatibility module's
wiring. `just verify` includes the same gate. Generator `--check` commands
remain the proof that complete checked-in DTO output is current.

## Operations API evolution

Compatible additions stay under `/operations/v1`. Any breaking HTTP change
requires a new `/operations/vN` base path, a new OpenAPI document, and a new
matrix compatibility line before SDKs adopt it. Product contract-format and
engine API evolution are recorded explicitly and do not inherit the Cargo
workspace patch version automatically.

See [Generated client policy](generated-clients.md) for DTO generation,
transport boundaries, and publishing rules.
