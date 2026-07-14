---
title: Agent skills
description: Repository skills that teach Codex and Claude Code to operate, author, and test with Templiqx.
---

# Agent skills

Templiqx ships three repository-local skills:

| Skill | Use |
| --- | --- |
| `use-templiqx` | Operate packages, contracts, document migration, rendering, and artifacts through MCP or CLI |
| `author-templiqx-contracts` | Create or repair strict `templiqx/v1alpha1` contracts |
| `test-templiqx-packages` | Validate packages, run eval fixtures, and report diagnostics and fingerprints |

The canonical files live under `.agents/skills/`, which Codex scans from the working directory through the repository root. Each skill's optional `agents/openai.yaml` supplies Codex app UI metadata. See the official [Codex skills documentation](https://developers.openai.com/codex/skills/). `.claude/skills/` contains relative symlinks to the same files, so Claude Code uses the identical instructions without duplicated copies drifting apart.

Invoke a skill explicitly with `$skill-name` in Codex or `/skill-name` in Claude Code. Natural-language requests can also trigger a matching skill description.

```text
Codex: $use-templiqx discover the packages and inspect the greeting contract.
Claude Code: /author-templiqx-contracts create a typed contract for this interaction.
Claude Code: /test-templiqx-packages validate demo and run all eval fixtures.
```

The skills prefer the Templiqx MCP server when it is configured and fall back to the repository CLI. Both surfaces call the same canonical application service and return the same operation envelopes.
