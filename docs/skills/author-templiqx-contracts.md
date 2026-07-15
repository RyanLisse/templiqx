---
title: author-templiqx-contracts
description: Create, repair, and validate strict templiqx/v1alpha1 contracts through Templiqx operations — typed inputs, bounded content nodes, deterministic expressions, capability profiles.
---


Create, edit, explain, and repair strict `templiqx/v1alpha1` YAML contracts and
package-local partials. Use this skill for prompt contracts with typed inputs and
context, components, deterministic expressions, structured output schemas,
capabilities, extensions, includes, or validation diagnostics. Canonical
instructions:
[`.agents/skills/author-templiqx-contracts/SKILL.md`](https://github.com/RyanLisse/templiqx/tree/main/.agents/skills/author-templiqx-contracts).

## Author safely

Write and validate through Templiqx operations rather than editing package files
behind the application when `put_contract` is available. A contract defines **exactly
one** model interaction: typed `inputs`, typed `context`, ordered messages, explicit
capability requirements, and a bounded output JSON Schema. Use only the supported
content nodes — `text`, `interpolate`, `when`, `for_each`, `component`, `include` —
and keep expressions deterministic (references, JSON literals, equality, negation,
`&&`, `||`). See the
[`references/contract-checklist.md`](https://github.com/RyanLisse/templiqx/tree/main/.agents/skills/author-templiqx-contracts/references/contract-checklist.md)
before writing one from scratch, and the [contract format](../contracts/v1alpha1) grammar.

## Smart CLI/MCP use

`put` takes the contract source as a **positional file path**, and overwriting an
existing contract needs `--expected-fingerprint`. Compilation needs an explicit
capability profile and fails closed if the contract requires more.

### Recipe — put → explain → validate → compile

```sh
R="--root examples/packages"
cargo run -q -p templiqx-cli -- $R put demo greeting ./greeting.yaml   # write source
cargo run -q -p templiqx-cli -- $R explain demo greeting               # graph + fix hints
cargo run -q -p templiqx-cli -- $R validate demo greeting              # ok envelope?
cargo run -q -p templiqx-cli -- $R compile demo greeting --capability text
```

Over MCP: `put_contract` → `explain_contract` → `validate_contract` →
`compile_contract` (`{ "package": "demo", "contract": "greeting", "capabilities": ["text"] }`).

## Repair, don't bypass

For undefined components or unresolved references, call `explain_contract` and follow
its fix hints. For capability failures, change the target profile or the requirement
*explicitly* — never silently drop a requirement. For schema failures, reduce to the
supported subset instead of bypassing validation. Require an `ok` validation envelope
and a successful compile before claiming a contract is ready.
