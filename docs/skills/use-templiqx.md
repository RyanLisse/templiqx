---
title: use-templiqx
description: Operate Templiqx packages, contracts, migration, rendering, and artifacts over MCP or the CLI — the same canonical operations, either surface.
---


Operate the Templiqx application: discover packages, inspect or compile contracts,
execute interactions, migrate legacy DOCX templates, render documents, and manage
workspace artifacts. Use this skill when an agent needs to **use** Templiqx, not
change its source. Canonical instructions:
[`.agents/skills/use-templiqx/SKILL.md`](https://github.com/RyanLisse/templiqx/tree/main/.agents/skills/use-templiqx).

## The rule

Prefer the `templiqx` MCP server; fall back to `cargo run -q -p templiqx-cli --`.
MCP and CLI expose the same operations and the same `OperationEnvelope`. Read the
envelope `ok` field — never infer success from a process exit code alone.

## Smart CLI use

`--root <packages-dir>` and `--json` are **global** flags; exit codes are `0` ok /
`2` product-diagnostic failure / `1` CLI-IO failure. A few non-obvious details make
or break a run:

- `execute` **requires** `--fixture-output <file>` (the deterministic receipt lands there).
- Lifecycle mutations are compare-and-swap: pass `--expected-fingerprint <fp>` and re-read on `TQX_CAS_CONFLICT`.
- Capability profiles are repeatable: `--capability text --capability json_output`; compilation fails closed if a required capability is missing.
- `render` previews compiled messages with no model call; `execute` performs the call.

### Recipe — inspect → validate → render

```sh
R="--root examples/packages"
cargo run -q -p templiqx-cli -- $R --json catalog          # authoritative op list
cargo run -q -p templiqx-cli -- $R discover                # packages
cargo run -q -p templiqx-cli -- $R inspect demo greeting   # structure + refs
cargo run -q -p templiqx-cli -- $R validate demo greeting  # ok envelope?
cargo run -q -p templiqx-cli -- $R render demo greeting --capability text
```

Over MCP the same flow is `catalog` → `discover_packages` → `inspect_contract` →
`validate_contract` → `render_contract` (`{ "package": "demo", "contract": "greeting", "capabilities": ["text"] }`).

The full operation table and MCP/CLI name mapping is in the skill's
[`references/operations.md`](https://github.com/RyanLisse/templiqx/tree/main/.agents/skills/use-templiqx/references/operations.md).
