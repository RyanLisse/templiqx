---
title: Agent skills
description: Download and use the three Templiqx repository skills so Codex and Claude Code operate, author, and test packages smartly over MCP or the CLI.
---

# Agent skills

Templiqx ships three repository-local **skills** — self-contained instruction
bundles that teach an agent to drive Templiqx through its MCP tools or CLI, using
the *same* canonical service a human would. They exist so an agent makes smart,
grounded use of the capability catalog instead of guessing command syntax or
inventing a second code path.

| Skill | Use it to | Page |
| --- | --- | --- |
| `use-templiqx` | Operate packages, contracts, migration, rendering, and artifacts | [use-templiqx](use-templiqx) |
| `author-templiqx-contracts` | Create, repair, and validate strict `templiqx/v1alpha1` contracts | [author-templiqx-contracts](author-templiqx-contracts) |
| `test-templiqx-packages` | Validate packages, run eval fixtures, and report diagnostics + fingerprints | [test-templiqx-packages](test-templiqx-packages) |

## MCP first, CLI fallback

Every skill follows one rule: **prefer the `templiqx` MCP server when its tools are
connected; otherwise run the CLI** (`cargo run -q -p templiqx-cli -- …`). MCP tool
names match CLI commands and the application method one-to-one (`validate_contract`,
`compile_contract`, `run_eval`, …), so the same operation is traceable across all
three surfaces. See the [capability map](../architecture/capability-map) for the full
1:1 table and [agent-native guide](../guides/agent-native) for connecting an agent
over MCP.

## Where the skills live

Canonical copies live under [`.agents/skills/`](https://github.com/RyanLisse/templiqx/tree/main/.agents/skills),
which Codex scans from the working directory up to the repository root. Each skill's
optional `agents/openai.yaml` supplies Codex app-UI metadata (see the official
[Codex skills docs](https://developers.openai.com/codex/skills/)). `.claude/skills/`
holds relative symlinks to the same files, so Claude Code and Codex read identical
instructions with no drifting copies.

Each skill is a directory:

```text
.agents/skills/<name>/
  SKILL.md              # the instructions the agent loads
  references/*.md       # concrete syntax, checklists, reporting formats
  agents/openai.yaml    # Codex app metadata
```

## Download the skills

The skills are plain files in the repo. To use them outside a full clone, sparse-checkout just the skills directory:

```sh
git clone --no-checkout --depth 1 https://github.com/RyanLisse/templiqx.git
cd templiqx
git sparse-checkout set .agents/skills
git checkout
# skills are now in .agents/skills/ — copy them into your agent's skills path
```

Or fetch a single skill's instructions directly:

```sh
curl -sSL https://raw.githubusercontent.com/RyanLisse/templiqx/main/.agents/skills/use-templiqx/SKILL.md
```

## Invoke a skill

Reference the skill by name; the agent loads its `SKILL.md` and follows it.

```text
Codex:        $use-templiqx discover packages and inspect the greeting contract
Claude Code:  /author-templiqx-contracts create a typed contract for this interaction
Claude Code:  /test-templiqx-packages validate the demo package and run all eval fixtures
```

Because both surfaces resolve to the same `TempliqxService`, you get identical
envelopes, diagnostics, and fingerprints whether the skill used MCP or the CLI.
