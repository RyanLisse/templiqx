---
date: 2026-07-12
topic: templiqx-best-in-class-template-engine-gap-analysis
origin:
  - docs/brainstorms/2026-07-11-templiqx-ai-native-template-engine-poc-requirements.md
  - docs/plans/2026-07-12-templiqx-production-readiness-without-crm3.md
---

# Templiqx vs Best-in-Class Template Engines — Gap Analysis

## Summary

Templiqx is not a classical text templating engine; it is a **typed AI-interaction contract compiler** with bounded content rendering and a narrow legacy-DOCX bridge. Compared across logic-less, full-featured, document-native, and AI-native categories, it leads on schema validation, deterministic compilation, evidence-grounded conformance, and human/agent parity — but lags on composition (includes, cross-package reuse), output breadth (HTML/PDF/ODT), i18n, custom filters, live streaming execution, and package dependency resolution. Pre-CRM3 production readiness (~99%) closed trust, deploy, and design-spike gaps; the next wave should deepen **AI-contract identity** without becoming a Handlebars/Jinja clone.

## Problem Frame

Blinqx teams evaluating Templiqx against Handlebars, Jinja2, Docmosis, or LangChain prompt templates will ask two questions: (1) can it replace our document and prompt templating stack, and (2) what must improve before CRM3 production? The POC and pre-CRM3 readiness plan prove the core hypothesis — portable typed contracts, deterministic fake execution, CRM3 synthetic conformance, DOCX V5 slice, package fingerprints, signing stub, and host handoff docs. Remaining gaps are not random omissions; most are **intentional boundary choices** (no arbitrary code, no orchestration, no provider SDKs in core). This analysis maps best-in-class feature dimensions to three tiers: must-have within Templiqx identity, nice-to-have document parity, and explicitly out of identity.

## Comparative Landscape

| Dimension | Logic-less (Handlebars, Mustache, Liquid) | Full-featured (Jinja2, Twig, Nunjucks) | Document-native (Docmosis, Carbone, docxtemplater, MERGEFIELD) | AI-native (LangChain, DSPy, structured output) | Templiqx today |
|-----------|-------------------------------------------|--------------------------------------|----------------------------------------------------------------|------------------------------------------------|----------------|
| Syntax: variables | `{{x}}` | `{{ x }}` + rich paths | merge fields, `$data.*` | prompt slots, f-strings | typed `interpolate` + schema-checked refs |
| Conditionals / loops | limited / none | `if`/`for`, macros | table loops, sections | chain steps external | `when`, `for_each` (bounded) |
| Includes / partials | `{{> partial}}` | `{% include %}`, inheritance | sub-documents, includes | prompt partials | in-contract `component` only |
| Filters / helpers | built-in + registerable | extensive + custom | formatters | output parsers | 4 fixed filters (`trim`, `lower`, `upper`, `json`) |
| Output formats | HTML, text, email | HTML + many | DOCX, PDF, ODT, XLSX | text/JSON | compiled messages + DOCX V5 slice |
| Schema / type safety | none | optional | data binding | Pydantic / JSON schema | JSON Schema inputs, context, output (core) |
| i18n / l10n | community helpers | gettext, filters | locale formatters | rare | none |
| Package / modules | npm/gems | pip/composer | template libraries | pip packages | `templiqx.yaml` manifest; no dependency resolver |
| Signing / trust | none | none | vendor trust | none | fingerprint + signing stub (ADR) |
| Compile cache | minimal | bytecode cache | server cache | minimal | deterministic fingerprints; no incremental cache |
| Streaming | N/A | N/A | batch render | token streams | ADR only; sync `execute` |
| Diagnostics | runtime errors | template trace | field reports | validation errors | stable codes, spans, JSON envelope |
| Security | logic-less = safer | sandbox varies | no code exec ideal | injection risk | no arbitrary code; strict YAML |
| Testing | snapshot | snapshot | golden docs | eval harness | package evals + CRM3 conformance |
| AI-specific | none | none | none | tools, multi-turn | structured output, capability gating, evidence grounding, trace receipts |

**Positioning verdict:** Templiqx should not chase full Jinja parity. It should close gaps that strengthen **typed, auditable, agent-operable AI contracts** and measured document compatibility.

## Gap Tiers

### Tier 1 — Must-have for CRM3 / production (within Templiqx identity)

| Gap | Best-in-class reference | Templiqx state | Why it matters |
|-----|-------------------------|----------------|----------------|
| Live streaming execution path | LangChain streaming, OpenAI SSE | ADR accepted; trait not extended | Host adapters need progressive output without a second receipt shape |
| Tool-contract references | DSPy signatures, shared OpenAPI tools | ADR only | Duplicated tool schemas across contracts do not scale |
| Package dependency locks | npm lockfiles, pip constraints | brainstorm R17; not implemented | Reproducible packages across opcos need pinned deps |
| Cross-package composition | Jinja imports, npm packages | manifest inventories only | Reusable components across packages |
| Bounded custom filters | Liquid register_filter | 4 hardcoded filters | Formatting (dates, numbers, casing) without arbitrary code |
| File-level includes in content | `{% include %}`, partials | `component` node only | Larger prompt/document templates need modular authoring |
| Production package signing | cosign, npm provenance | stub + ADR | Tamper evidence before publication |
| IDE-grade diagnostics | Jinja trace, ESLint | CLI/MCP `explain`; no LSP | Agent and developer edit loops need source-addressed help |

### Tier 2 — Nice-to-have parity (document engines)

| Gap | Reference | Notes |
|-----|-----------|-------|
| HTML / plain-text render adapter | Carbone HTML, Handlebars | Useful for email and web snippets tied to same contract data |
| PDF output | Docmosis, Carbone | Host-owned or adapter; not core compiler |
| ODT dialect | LibreOffice templates | Deferred in brainstorm; compatibility port must allow it |
| i18n / pluralization / locale formats | gettext, Liquid locale | Host may supply locale context; core needs format filters |
| Compile artifact cache | Jinja bytecode | Performance; must not break fingerprint determinism |
| Visual template editor | Docmosis, Word | Host/product surface; Templiqx stays file-canonical |
| Broader DOCX V5 coverage | docxtemplater loops/images | Expand fixture-measured corpus, not "supports everything" |

### Tier 3 — Explicitly out of identity (defer)

| Feature | Why defer |
|---------|-----------|
| Full Handlebars/Jinja/VTL syntax compatibility | Violates typed contract model and safe-expression boundary |
| Arbitrary script filters (JS, BeanShell, Rust) | brainstorm R4/R22 — unsafe legacy never executes |
| Agent orchestration, RAG, durable workflows | Host-owned; brainstorm R10 |
| Model gateway, credentials, tenant policy | Host-owned |
| Hosted registry / marketplace | Post-POC product surface |
| Pixel-perfect rendering / full OOXML fidelity | Compatibility = measured parity, not WYSIWYG |
| WASM / incremental virtual-DOM rendering | brainstorm scope boundary until adoption proven |
| Becoming a general HTML site generator | Outside AI-contract compiler mission |

## Requirements (prioritized improvements)

### AI-contract compiler depth

- **R1.** Implement the streaming `RuntimeAdapter` extension per `docs/architecture/adr-streaming-runtime-port.md` with a deterministic mock that emits `StreamEvent` sequences and preserves final `ExecutionReceipt` parity with `execute`.
- **R2.** Implement tool-contract references per `docs/architecture/adr-tool-contract-refs.md`: package-level `tool_contracts` table, compile-time `$ref` resolution, fail-closed diagnostics.
- **R3.** Add package dependency declarations with content-addressed lock verification at `validate_package` — no online registry required.
- **R4.** Support cross-package component and include resolution from declared dependencies with cycle detection and fingerprint pinning.
- **R5.** Add a bounded custom-filter registry: filters declare input/output JSON Schema subsets; no user-defined code execution.
- **R6.** Add file-level `include` content nodes resolving paths relative to package root with the same path-safety rules as `put_contract`.

### Authoring, diagnostics, and trust

- **R7.** Extend `explain_contract` and validation diagnostics with fix suggestions and component/include trace graphs suitable for IDE or agent consumption.
- **R8.** Promote package signing from stub to CI-verified round-trip (cosign-compatible) without blocking unsigned dev packages by default.
- **R9.** Document and test locale-aware formatting via `context.locale` plus approved filters (dates, numbers) — no gettext engine in core.

### Document output (measured compatibility)

- **R10.** Ship a minimal HTML/plain-text `DocumentRenderer` adapter driven by the same merge data model as DOCX V5.
- **R11.** Expand legacy corpus with additional V5 edge cases (images, nested sections) under fixture-measured claims only.
- **R12.** Publish an ODT compatibility ADR and import-only detection path; full render remains deferred until corpus exists.

### Explicit non-requirements

- **R13.** Templiqx must not add a second template language syntax (Jinja/Handlebars compatibility layer).
- **R14.** Templiqx must not embed provider SDKs or live model calls in default CLI/MCP composition.

## Actors

- **A1. Contract author (human)** — writes YAML contracts, components, and package manifests.
- **A2. Agent operator** — discovers, validates, compiles, tests, and migrates via CLI/MCP with structured envelopes.
- **A3. Host integrator** — wires `RuntimeAdapter`, document renderers, signing policy, and locale context.
- **A4. Conformance CI** — runs package evals, CRM3 scenarios, and portability gates.

## Key Flows

- **F1. Modular contract authoring**
  - **Trigger:** Author splits a large extraction prompt into shared partials.
  - **Actors:** A1, A2
  - **Steps:** Declare dependency → resolve `include`/`component` → validate types → compile with stable fingerprint.
  - **Covered by:** R4, R6, R7

- **F2. Streaming execution with deterministic receipt**
  - **Trigger:** Host adapter streams tokens during drafting.
  - **Actors:** A3, A4
  - **Steps:** `execute_streaming` emits deltas → terminal `Complete` carries same receipt as `execute` → conformance asserts fingerprint rules.
  - **Covered by:** R1

- **F3. Trusted package consumption**
  - **Trigger:** Second opco imports a signed contract package.
  - **Actors:** A3, A4
  - **Steps:** `validate_package` verifies lock + signature → compile → run evals.
  - **Covered by:** R3, R8

## Acceptance Examples

- **AE1. Streaming parity** — Covers F2 / R1. Given a mock scenario with three `Delta` events, when `execute_streaming` runs, then the final `Complete` receipt matches a non-streaming `execute` receipt for fingerprints and `output_schema_valid`.
- **AE2. Shared tool schema** — Covers R2. Given two contracts referencing `tool_contract:search_customers` with the same fingerprint, when either compiles, then `CompiledInteraction.extensions` contains identical resolved tool schema; a tampered fingerprint yields `TQX_TOOL_CONTRACT_REF_UNRESOLVED`.
- **AE3. Cross-package component** — Covers R4 / F1. Given package B depending on package A's `salutation` component, when B invokes it with typed args, then validation passes; a cyclic dependency fails at `validate_package`.
- **AE4. Bounded date filter** — Covers R5 / R9. Given `context.locale=nl-NL` and a date field, when `format_date` filter applies, then output matches fixture without executing scripts.
- **AE5. HTML render** — Covers R10. Given CRM3 draft JSON and an HTML template, when `render_document` runs, then output matches golden file; path traversal in template name fails closed.

## Success Criteria

- Tier-1 gaps R1–R8 have conformance or unit tests proving behavior without a live CRM3 host.
- CRM3 grounded-evidence checks and deterministic fake fingerprint parity remain green.
- `./scripts/check-boundaries.sh` passes; no provider SDKs or mocks in default composition.
- Documentation states measured compatibility scope for DOCX/HTML; no "supports all Word features" claim.
- Host-blocked items (live ModelGateway, real opco data, tenant auth) remain explicitly documented as host-owned.

## Scope Boundaries

### In scope (next wave)

- Implement accepted ADRs (streaming, tool-contract refs, package trust hardening)
- Package composition (deps, includes, cross-package components)
- Bounded filter extension and locale formatting foundation
- HTML/plain-text adapter and corpus expansion under fixture discipline

### Deferred for later

- PDF and ODT render adapters (after ADR + corpus)
- Compile artifact incremental cache
- Visual editor, hosted registry, WASM packaging
- Real second Blinqx opco package (host/product gate)
- Full V1/V2 migration (detect/report first; migrate when corpus ready)

### Outside this product's identity

- Jinja/Handlebars/VTL syntax compatibility
- Arbitrary code execution in templates
- Agent orchestration, RAG, workflow engine
- Model gateway ownership
- General-purpose static site generation

## Key Decisions

- **Compare categories, not one engine** — gaps are dimensional; positioning stays AI-contract-first.
- **Tier 1 before Tier 2** — streaming, tool refs, deps, and composition unblock hosts and agents; HTML/i18n follow.
- **Measured document claims only** — expand DOCX/HTML via fixture corpus, matching R24 pattern.
- **Standalone-first sequencing** — all Tier-1 items can start without CRM3 ModelGateway or production fixtures.
- **ADRs are the implementation spec** — streaming and tool-contract refs move from design to code before new greenfield designs.

## Alternatives Considered

### Become a Jinja-compatible superset

Familiar authoring syntax, but destroys typed-schema guarantees, invites arbitrary expressions, and duplicates ecosystem tools without AI-contract differentiation.

### Freeze at POC surface

Low carrying cost, but hosts will duplicate tool schemas, streaming shims, and package wiring — fragmentation across Blinqx opcos.

### Outsource document rendering entirely to host

Clean boundaries, but CRM3 needs a portable merge-data path from structured draft to DOCX/HTML; a thin adapter port in Templiqx remains necessary.

## Dependencies / Assumptions

- Pre-CRM3 readiness plan items U1–U9 remain complete (signing stub, ADRs, synthetic opco, legacy corpus baseline).
- Host teams consume streaming and signing via `docs/guides/host-integration.md`; live proof still needs Basenet adapter.
- Locale and PDF needs are confirmed through CRM3 pilot feedback, not speculative full gettext.

## Outstanding Questions

### Resolve Before Planning

None blocking — Tier-1 scope is derivable from ADRs and brainstorm R1–R30.

### Deferred to Planning

- **[Affects R5]** Which date/number format filters ship in v1 vs remain host extensions?
- **[Affects R10]** HTML adapter: minimal mustache-like `{{field}}` in HTML only, or reuse contract content nodes?
- **[Affects R3]** Dependency lock format: cargo-style vs npm-style manifest — planning chooses one additive shape.

## Next Steps

→ `docs/plans/2026-07-12-001-feat-template-engine-parity-plan.md` for implementation units U1–U8.
