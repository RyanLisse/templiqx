# Agent-Native Architecture Audit — Templiqx

**Date:** 2026-07-12  
**Scope:** Rust workspace — CLI (`templiqx-cli`), MCP server (`templiqx-mcp`), capability catalog, application layer, conformance tests  
**Overall agent-native score: 66%** (⚠️ Partial — strong foundation, gaps in agent context and entity completeness)

---

## Overall Score Summary

| Core Principle | Score | Percentage | Status |
|----------------|-------|------------|--------|
| Action Parity | 13/14 | 93% | ⚠️ |
| Tools as Primitives | 12/13 | 92% | ✅ |
| Context Injection | 2/6 | 33% | ❌ |
| Shared Workspace | 2/2 | 100% | ✅ |
| CRUD Completeness | 0/4 entities full | 38% ops | ❌ |
| UI Integration (CLI/MCP envelopes + FS) | 3/4 | 75% | ⚠️ |
| Capability Discovery | 2/7 | 29% | ❌ |
| Prompt-Native Features | 7/11 | 64% | ⚠️ |

**Overall Agent-Native Score: 66%**

---

## Top 3 Gaps

1. **Context injection (33%)** — MCP exposes only a one-line `with_instructions` string; no dynamic injection of package root contents, workspace artifacts, recent receipts, or capability vocabulary. Agents must cold-call `discover_packages` with no session bootstrap.

2. **CRUD completeness (0/4 entities with full CRUD)** — Contracts lack delete; workspace artifacts lack read/list/delete; package scaffolding (`create_package`) exists only in Rust tests, not in the 13-op catalog. Agents can write but cannot fully manage lifecycle.

3. **Capability discovery (29%)** — No agent onboarding, suggested workflows, or `catalog` MCP tool (CLI-only). Discovery relies on external docs (`docs/guides/cli.md`, `openwiki/`) and MCP `list_tools`.

---

## Top 10 Recommendations by Impact

| Priority | Action | Principle | Effort |
|----------|--------|-----------|--------|
| 1 | Add `list_workspace_artifacts` + `read_artifact` primitives for workspace outputs | CRUD / Context | Medium |
| 2 | Add `delete_contract` (and optionally manifest line removal) with CAS safety | CRUD | Medium |
| 3 | Expose `catalog` as 14th MCP tool or enrich `get_info` with full capability list + vocabulary | Discovery / Parity | Low |
| 4 | Inject dynamic context resource: packages under root, workspace path, default capabilities | Context | Medium |
| 5 | Add agent onboarding doc section + MCP instructions with example tool sequences (discover→validate→compile→execute) | Discovery | Low |
| 6 | Extend `explain_contract` with fix suggestions, component graph (per gap analysis R7) | Discovery / Context | Medium |
| 7 | Expose `create_package` in catalog for agent-initiated package bootstrap | CRUD / Parity | Low |
| 8 | Add `complete_task` or receipt-visibility pattern for multi-step host orchestration | UI Integration | Medium |
| 9 | Split `test_package` into `list_evals` + `run_eval` for finer agent composition | Tools as Primitives | Medium |
| 10 | Ship streaming execution on `RuntimeAdapter` with same receipt fingerprints (planned ADR) | UI Integration / Prompt | High |

---

## What's Working Excellently (Top 5)

1. **Rust / CLI / MCP parity** — `rust_cli_and_in_memory_mcp_have_crm3_capability_parity` in `crates/templiqx-conformance/tests/crm3.rs` asserts byte-identical envelopes across all 13 catalog operations.

2. **Atomic capability catalog** — Single `CAPABILITY_CATALOG` in `crates/templiqx-application/src/lib.rs`; MCP tool names match exactly (`TOOL_CATALOG` in `crates/templiqx-mcp/src/lib.rs`).

3. **Prompt-native AI contracts** — CRM3 contracts (e.g. `examples/crm3/contracts/bli-61-date-term-extraction.yaml`) define system/user messages, JSON Schema outputs, and eval fixtures in YAML — behavior changes without Rust refactors.

4. **Shared filesystem workspace** — Package store (read + CAS write) and artifact workspace (writable outputs) are shared by CLI and MCP via `templiqx_local::compose()`; no agent sandbox (`crates/templiqx-local/src/lib.rs`, `docs/architecture/poc.md`).

5. **Structured agent-native diagnostics** — `OperationEnvelope` + `StructuredEnvelope` return stable codes, spans, fingerprints; domain failures are not MCP protocol errors (`crates/templiqx-mcp/src/lib.rs`).

---

## Per-Principle Detailed Sections

### 1. Action Parity — 13/14 (93%) ⚠️

**Adaptation:** CLI = human/operator; MCP = agent; no web UI.

| Action | Location | Agent Tool | Status |
|--------|----------|------------|--------|
| catalog | `crates/templiqx-cli/src/main.rs` | — (use `list_tools`) | ⚠️ Gap |
| discover | CLI + MCP | `discover_packages` | ✅ |
| inspect | CLI + MCP | `inspect_contract` | ✅ |
| put | CLI + MCP | `put_contract` | ✅ |
| validate (contract) | CLI + MCP | `validate_contract` | ✅ |
| validate (package) | CLI + MCP | `validate_package` | ✅ |
| compile | CLI + MCP | `compile_contract` | ✅ |
| render | CLI + MCP | `render_contract` | ✅ |
| execute | CLI + MCP | `execute_contract` | ✅ |
| test | CLI + MCP | `test_package` | ✅ |
| diff | CLI + MCP | `diff_contract` | ✅ |
| explain | CLI + MCP | `explain_contract` | ✅ |
| migrate | CLI + MCP | `migrate_legacy` | ✅ |
| render-document | CLI + MCP | `render_document` | ✅ |
| crm3-conformance | CLI only | — | ✅ By design |
| create_package | Rust tests only | — | ❌ Gap |

**Evidence:** Parity test at `crm3.rs:405–646`. Architecture explicitly excludes trace receipt composition from the catalog (`docs/architecture/poc.md:64–68`).

**Missing:** `catalog` MCP tool; `create_package` on any surface.

---

### 2. Tools as Primitives — 12/13 (92%) ✅

| Tool | File | Type | Reasoning |
|------|------|------|-----------|
| discover_packages | `templiqx-mcp/src/lib.rs:323` | PRIMITIVE | List packages |
| inspect_contract | `:329` | PRIMITIVE | Read one contract |
| put_contract | `:338` | PRIMITIVE | CAS write |
| validate_contract | `:347` | PRIMITIVE | Validate one |
| validate_package | `:356` | PRIMITIVE | Validate inventory |
| compile_contract | `:365` | PRIMITIVE | Deterministic compile |
| render_contract | `:374` | PRIMITIVE | Deterministic render |
| execute_contract | `:383` | PRIMITIVE | Single interaction |
| migrate_legacy | `:392` | PRIMITIVE | Single adapter call |
| render_document | `:401` | PRIMITIVE | Single render |
| test_package | `:410` | WORKFLOW-LITE | Runs all package evals in one call |
| diff_contract | `:419` | PRIMITIVE | Compare two |
| explain_contract | `:427` | PRIMITIVE | Metadata introspection |

No CRM3 orchestration tool — conformance stays at host/CLI boundary (correct for agent-native granularity).

---

### 3. Context Injection — 2/6 (33%) ❌

| Context Type | Injected? | Location | Notes |
|--------------|-----------|----------|-------|
| Available resources | Partial | `discover_packages` tool | Not in system prompt |
| User preferences | N/A | — | Host-owned |
| Recent activity | No | — | No session state |
| Available capabilities | Partial | MCP `list_tools`, one-line instructions | Minimal |
| Session history | No | — | Stateless stdio MCP |
| Workspace state | No | — | Agent must infer paths |

**Evidence:** Only injection point is `get_info().with_instructions(...)` at `templiqx-mcp/src/lib.rs:443`.

---

### 4. Shared Workspace — 2/2 (100%) ✅

| Data Store | User Access | Agent Access | Shared? |
|------------|-------------|--------------|---------|
| Package store (`FilesystemPackageStore`) | CLI `--root` | MCP arg / `TEMPLIQX_PACKAGES_ROOT` | ✅ |
| Artifact workspace (`.templiqx-workspace/` or explicit) | CLI `--workspace` | MCP `workspace` param | ✅ |

**Evidence:** Workspace tests in `templiqx-mcp/tests/workspace.rs`, `templiqx-cli/tests/workspace.rs`. Package inputs read-only; generated artifacts go to workspace (`docs/architecture/poc.md:36–44`). No separate agent data space.

---

### 5. CRUD Completeness — 0/4 entities full (38% ops) ❌

| Entity | Create | Read | Update | Delete | Score |
|--------|--------|------|--------|--------|-------|
| Contract | `put_contract` | `inspect_contract` | `put_contract` + CAS | — | 3/4 |
| Package | `create_package` (Rust only) | `discover_packages` | — | — | 1/4 |
| Workspace artifact | `render_document` | — | — | — | 1/4 |
| Package manifest | — | via discover | — | — | 1/4 |

**Evidence:** `PackageStore` trait (`templiqx-ports/src/lib.rs:76–105`) has no delete. `create_package` at `templiqx-local/src/lib.rs:603`.

---

### 6. UI Integration — 3/4 (75%) ⚠️

*N/A for web UI; assessed via CLI JSON / MCP structured content / filesystem coherency.*

| Agent Action | Visibility Mechanism | Immediate? | Notes |
|--------------|---------------------|------------|-------|
| execute_contract | Envelope + receipt in response | ✅ | Fingerprints in envelope |
| put_contract | Filesystem write under package root | ⚠️ | Must re-inspect; no push |
| render_document | Workspace file write | ⚠️ | No read-back tool |
| validate/compile | Structured envelope | ✅ | Same for CLI and MCP |

No SSE/streaming, file watching, or LSP integration yet (acknowledged in gap analysis).

---

### 7. Capability Discovery — 2/7 (29%) ❌

| Mechanism | Exists? | Location | Quality |
|-----------|---------|----------|---------|
| Onboarding flow | No | — | — |
| Help documentation | Yes | `docs/guides/cli.md`, `openwiki/workflows.md` | Good for humans |
| Capability hints | Partial | MCP tool `description` fields | Brief |
| Agent self-describes | Minimal | MCP instructions one line | Weak |
| Suggested prompts | No | — | — |
| Empty state guidance | No | — | — |
| Slash commands | Partial | CLI `--help` only | Not MCP-native |

MCP `list_tools` with JSON Schema I/O is the strongest discovery path (`templiqx-mcp/src/lib.rs:491–531`).

---

### 8. Prompt-Native Features — 7/11 (64%) ⚠️

| Feature | Defined In | Type | Notes |
|---------|------------|------|-------|
| BLI-61 extraction behavior | Contract YAML | PROMPT | Messages + output_schema |
| BLI-62 drafting | Contract YAML | PROMPT | Same |
| Eval assertions | Contract YAML evals | PROMPT | Schema + fixture refs |
| Capability requirements | Contract YAML | PROMPT | Host enforces profile |
| Validation / compile engine | `templiqx-core` | CODE | Appropriate |
| Content node rendering | `templiqx-core` | CODE | Bounded interpreter |
| CRM3 trace receipt | Conformance harness | CODE | Host orchestration |
| Actor approval boundary | `crm3_actor_boundary.rs` | HOST CODE | Correct separation |
| V5 DOCX migration | `templiqx-docx-v5` | CODE | Adapter |
| Package fingerprints | Core + manifest | CODE | Determinism |
| test_package runner | Application | CODE | Test harness |

**Verdict:** AI outcomes are prompt-native (YAML contracts); infrastructure correctly stays in code. Changing extraction/drafting behavior = edit contract YAML, not Rust.

---

## Architecture-Specific Notes

- **Host boundary is correct:** Approval/idempotency in `crm3_actor_boundary.rs` is host policy, not a Templiqx capability gap — agents and humans share `execute_contract`; host gates agent direct-commit.
- **Receipt composition deliberately excluded** from catalog per `docs/architecture/poc.md` — agents compose multi-step flows via primitives (aligns with agent-native granularity).
- **Conformance parity is best-in-class** for a compiler POC — rare to have automated Rust/CLI/MCP envelope equality tests.

---

## Related Documents

- [Gap analysis](../brainstorms/2026-07-12-templiqx-best-in-class-template-engine-gap-analysis.md)
- [Capability map](../architecture/capability-map.md)
- [Host integration guide](../guides/host-integration.md)
- [CLI guide](../guides/cli.md)
