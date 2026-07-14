---
title: test-templiqx-packages
description: Validate Templiqx packages, run deterministic eval fixtures, compare fingerprints, and report diagnostics through MCP or the CLI.
---

# test-templiqx-packages

Validate Templiqx packages and contracts, discover and run deterministic eval
fixtures, compare fingerprints, inspect diagnostics, and verify rendered workspace
artifacts. Use this skill when testing a package, reviewing a contract change,
reproducing a fixture, or checking deterministic behavior. Canonical instructions:
[`.agents/skills/test-templiqx-packages/SKILL.md`](https://github.com/RyanLisse/templiqx/tree/main/.agents/skills/test-templiqx-packages).

## Test sequence

Confirm the package exists, validate it, then run the specific fixture (or the whole
suite only when package readiness is the ask). For determinism acceptance, repeat a
deterministic eval and compare the **request, output, and receipt fingerprints** — a
stable fingerprint across runs is the proof. Read the
[`references/reporting.md`](https://github.com/RyanLisse/templiqx/tree/main/.agents/skills/test-templiqx-packages/references/reporting.md)
for the required evidence format.

### Recipe — validate → list-evals → run-eval

```sh
R="--root examples/packages"
cargo run -q -p templiqx-cli -- $R discover                       # package present?
cargo run -q -p templiqx-cli -- $R validate demo                  # whole package (contract omitted)
cargo run -q -p templiqx-cli -- $R list-evals demo                # fixture ids
cargo run -q -p templiqx-cli -- $R --json run-eval demo greeting <fixture-id>
cargo run -q -p templiqx-cli -- $R test demo                      # full suite (when readiness is the ask)
```

Over MCP: `discover_packages` → `validate_package` → `list_evals` → `run_eval` →
`test_package`. `validate <package>` with the contract argument omitted validates the
entire package.

## Boundaries

- Application evals are not repository unit tests — do not substitute one for the other.
- Mock conformance results are not proof of a real provider integration.
- Don't run the expensive fresh-clone proof unless explicitly asked; it's a separate on-demand gate.
- Don't delete packages or artifacts while testing, and never expose provider or signing secrets in commands or reports.

State which package, contracts, and fixture IDs ran; report every envelope's `ok`
status, stable diagnostic codes, and fingerprints; and distinguish application-level
failures (`ok: false`, exit `2`) from CLI/MCP transport failures (exit `1`).
