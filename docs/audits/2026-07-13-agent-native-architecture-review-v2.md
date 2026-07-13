# Agent-Native Architecture Re-Audit (v2) — Templiqx

**Date:** 2026-07-13  
**Scope:** Rust workspace post parity work (commits `fad5b1b`, `78ce3ef`) — CLI, MCP server, 20-op capability catalog, conformance tests  
**Baseline:** [v1 audit (2026-07-12)](2026-07-12-agent-native-architecture-review.md) — **66% overall**  
**Overall agent-native score: 86%** (+20pp from v1) (✅ Strong — catalog parity achieved; CRUD and discovery polish remain)

---

## Overall Score Summary

| Core Principle | Score | % | Status | Δ vs v1 |
|----------------|-------|---|--------|---------|
| Action Parity | 20/20 | 100% | ✅ | +7pp |
| Tools as Primitives | 19/20 | 95% | ✅ | +3pp |
| Context Injection | 5/6 | 83% | ✅ | +50pp |
| Shared Workspace | 2/2 | 100% | ✅ | — |
| CRUD Completeness | 1/4 entities · 11/16 ops | 69% | ⚠️ | +31pp (ops) |
| UI Integration (CLI/MCP) | 4/5 | 80% | ⚠️ | +5pp |
| Capability Discovery | 6/7 | 86% | ✅ | +57pp |
| Prompt-Native Features | 8/12 | 67% | ⚠️ | +3pp |

**Overall Agent-Native Score: 86%** (was 66%)

---

## Can We Push to 100%?

**Answer: Partial — no for literal 100%; yes for ~92–95% within Templiqx identity.**

### Architectural ceiling (by design, not bugs)

- Stateless stdio MCP → no session history, no push/subscription UI
- Host-owned: recent activity, user preferences, trace receipt composition, CRM3 `crm3-conformance` orchestration
- No web UI → Principle 6 is envelope/FS coherency, not live UI sync
- `test_package` remains an intentional convenience wrapper (not a primitive)

### Achievable with P0/P1

Package/artifact delete, workspace resource in bootstrap, richer discovery docs, full 20-op parity harness, optional MCP prompts.

---

## Gap-to-100% (per principle)

### 1. Action Parity — 20/20 (100%) ✅

| Action | CLI | MCP tool | Status |
|--------|-----|----------|--------|
| All 20 `CAPABILITY_CATALOG` ops | ✅ | ✅ | ✅ |
| `crm3-conformance` | CLI only | — | ✅ by design |

**Evidence:** `CAPABILITY_CATALOG` in `crates/templiqx-application/src/lib.rs:106–127`; `TOOL_CATALOG` in `crates/templiqx-mcp/src/lib.rs:35`; 20 `#[tool]` handlers in `crates/templiqx-mcp/src/lib.rs:479–650`.

**Gap to 100%:** None for catalog ops. `crm3-conformance` stays CLI-only (host boundary).

---

### 2. Tools as Primitives — 19/20 (95%) ✅

| Tool | Type | Notes |
|------|------|-------|
| 19 catalog tools | PRIMITIVE | Single capability each |
| `test_package` | WORKFLOW-LITE | Runs all evals; mitigated by `list_evals` + `run_eval` |

**Gap to 100%:** Deprecate or demote `test_package` in agent docs; or accept wrapper as permanent (95% ceiling).

---

### 3. Context Injection — 5/6 (83%) ✅

| Context Type | Injected? | Location |
|--------------|-----------|----------|
| Available resources | ✅ | `templiqx://catalog`, `templiqx://packages` (`lib.rs:669–702`) |
| Available capabilities | ✅ | `catalog` tool + resources + instructions |
| Package/workspace bootstrap | ✅ partial | `agent_instructions()` (`:433–462`) — packages root + discovered names + empty-state |
| Workspace default path | ⚠️ partial | Default `.templiqx-workspace` not in instructions |
| User preferences | N/A | Host-owned |
| Session / recent activity | N/A | Stateless MCP |

**Gap to 100%:** `templiqx://workspace` resource; default workspace path in instructions; optional per-package contract summaries in resources (P2).

---

### 4. Shared Workspace — 2/2 (100%) ✅

Package store + artifact workspace shared by CLI/MCP via `templiqx_local::compose()`. No agent sandbox.

**Gap to 100%:** None.

---

### 5. CRUD Completeness — 1/4 entities full · 11/16 ops (69%) ⚠️

| Entity | C | R | U | D | Score |
|--------|---|---|---|---|-------|
| Contract | `put_contract` | `inspect_contract` | `put_contract` (CAS) | `delete_contract` | **4/4** |
| Package | `create_package` | `discover_packages` | — | — | 2/4 |
| Workspace artifact | `render_document` / execute | `read_artifact` + `list_workspace_artifacts` | overwrite via render | — | 3/4 |
| Package manifest | `create_package` | via `discover_packages` | indirect via contract ops | — | 2/4 |

**Evidence:** `PackageStore::create_package` / `delete_contract` (`crates/templiqx-ports/src/lib.rs:106–113`); `ArtifactWorkspace::list_artifacts` / `read_artifact` (`:137–151`); conformance in `crates/templiqx-conformance/tests/agent_native.rs`.

**Gap to 100%:** `delete_package`, manifest update (`put_manifest` / version bump), `delete_workspace_artifact`.

---

### 6. UI Integration — 4/5 (80%) ⚠️

| Agent action | Visibility | Immediate? |
|--------------|------------|------------|
| `execute_contract` | Envelope + `stream_events` when `stream: true` | ✅ |
| `validate` / `compile` | Structured envelope | ✅ |
| `render_document` | FS write + `list_workspace_artifacts` + `read_artifact` | ✅ |
| `put_contract` | FS write; must re-`inspect_contract` | ⚠️ |
| Live push / watch | None | ❌ (out of scope) |

**Evidence:** `stream_events` in `crates/templiqx-conformance/tests/streaming.rs:176–215`; workspace read/list in MCP tools `crates/templiqx-mcp/src/lib.rs:592–610`.

**Gap to 100%:** Artifact path hints in execute/render envelopes; resource refresh on `put_contract` (P2); SSE/LSP/file-watch explicitly out of scope.

---

### 7. Capability Discovery — 6/7 (86%) ✅

| Mechanism | Exists? | Location |
|-----------|---------|----------|
| Onboarding flow | ✅ | `docs/guides/cli.md` § Agent workflows (`:34–56`) |
| Help documentation | ✅ | `docs/guides/cli.md`, `openwiki/` |
| Capability hints | ⚠️ | MCP `description` fields — present, still brief |
| Agent self-describes | ✅ | `agent_instructions()` >200 chars (`lib.rs:755–759` test) |
| Suggested flows | ✅ | 3 flows in cli.md + instruction sequence |
| Empty-state guidance | ✅ | "No packages discovered — call create_package" |
| Slash commands | ⚠️ | CLI `--help` only; no MCP prompt surface |

**Gaps:** `docs/guides/agent-onboarding.md` does not exist (workflows live in `cli.md`); `docs/architecture/capability-map.md` was **stale** (listed 13 ops; updated in this pass to 20).

**Gap to 100%:** MCP prompts/`/help` equivalent; richer per-tool examples in schemas.

---

### 8. Prompt-Native Features — 8/12 (67%) ⚠️

| Feature | Defined in | Type |
|---------|------------|------|
| BLI-61/62 behavior, evals, capabilities | Contract YAML | PROMPT ✅ |
| `explain_contract` graph + `fix_hints` | Code → structured hints | HYBRID ✅ |
| Validation, compile, render engine | `templiqx-core` | CODE ✅ (appropriate) |
| `test_package` / eval runner | Application | CODE ✅ |
| CRM3 trace / actor approval | Host | HOST ✅ |
| DOCX V5 migration | Adapter | CODE ✅ |

**Gap to 100%:** More authoring behavior in contracts (filters, includes landed in template plan); eval criteria remain partly in Rust harness.

---

## Prioritized Roadmap to ~95%

| Priority | Action | Principle | Effort |
|----------|--------|-----------|--------|
| **P0** | `delete_workspace_artifact` with path confinement | CRUD | M |
| **P0** | `delete_package` (+ manifest cleanup) | CRUD | M |
| **P0** | `templiqx://workspace` resource + default path in instructions | Context | S |
| **P0** | Extend conformance parity to all 20 catalog ops (not just CRM3 subset) | Parity | M |
| **P0** | Update `docs/architecture/capability-map.md` to 20 ops | Discovery | S |
| **P1** | `update_package` / manifest version bump | CRUD | M |
| **P1** | Richer MCP tool descriptions + JSON schema examples | Discovery | S |
| **P1** | MCP prompt templates (`bootstrap`, `run-eval`) | Discovery | M |
| **P1** | Envelope hints: `workspace_artifact` paths post-execute/render | UI Integration | S |
| **P2** | Per-package contract summary resource | Context | M |
| **P2** | Resource refresh / subscription on `put_contract` | UI Integration | L |
| **P2** | Helm/kind smoke: MCP init + `catalog` + `discover_packages` | Discovery | S |

---

## Top 10 Recommendations (by impact)

1. **P0** — `delete_workspace_artifact` → CRUD artifact 4/4
2. **P0** — `delete_package` → package lifecycle completeness
3. **P0** — Workspace bootstrap in MCP instructions/resources
4. **P0** — Full 20-op Rust/CLI/MCP parity harness
5. **P0** — Refresh `capability-map.md` (was stale at 13 ops)
6. **P1** — Manifest update op for package entity
7. **P1** — MCP prompt templates for the three documented flows
8. **P1** — Post-op artifact path hints in envelopes
9. **P2** — Richer per-package MCP resources
10. **P2** — Document `test_package` as convenience-only in MCP instructions

---

## Top 5 Strengths

1. **20/20 catalog parity** — single `CAPABILITY_CATALOG` in `crates/templiqx-application/src/lib.rs`; MCP tool names match exactly (`TOOL_CATALOG` in `crates/templiqx-mcp/src/lib.rs`).
2. **Shared workspace 100%** — same FS for CLI and MCP; workspace confinement tested (`crates/templiqx-mcp/tests/workspace.rs`).
3. **MCP bootstrap** — dynamic instructions, `templiqx://catalog` + `templiqx://packages`, substantive onboarding test (`crates/templiqx-mcp/src/lib.rs`).
4. **Contract full CRUD** — CAS `put_contract` + `delete_contract` with manifest update (`crates/templiqx-conformance/tests/agent_native.rs`).
5. **`explain_contract` diagnostic graph** — `unresolved_references` + `fix_hints` (`Explanation` in `crates/templiqx-contracts/src/lib.rs`).

---

## Verdict

| Question | Answer |
|----------|--------|
| New overall score | **86%** (+20pp from 66%) |
| Parity plan target (~88%) | **Close** — CRUD + stale docs + partial workspace context hold it back |
| Can we hit 100%? | **No** (architectural ceiling ~92–95%) |
| Can we push further? | **Yes** — P0 CRUD + context + docs gets to ~90–92%; P1 discovery polish → ~93–95% |

---

## Related artifacts

- Parity implementation plan: `docs/plans/2026-07-12-002-feat-agent-native-parity-plan.md`
- v1 baseline: `docs/audits/2026-07-12-agent-native-architecture-review.md`
- Capability map (updated): `docs/architecture/capability-map.md`
