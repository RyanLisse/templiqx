---
date: 2026-07-11
topic: templiqx-ai-native-template-engine-poc
---

# Templiqx AI-Native Template Engine POC

## Problem Frame

Blinqx companies need a reusable way to define AI interactions without binding product code to one model provider, prompt syntax, runtime, or company-specific platform. Existing template engines are text- and document-oriented, weakly typed, difficult for agents to operate safely, and unable to express structured outputs, tools, capability requirements, provenance, and evaluations as one auditable contract.

Templiqx will be a standalone Rust product used first by Basenet CRM3. Basenet is the first conformance host, not the owner of Templiqx's architecture. The POC must prove that developers and agents can use the same portable contract through a Rust API, CLI, and MCP; that the contract can drive both typed legal extraction and AI-assisted document drafting; and that legacy document templates can be migrated without carrying unsafe legacy runtimes into the new core.

## Product Direction

Templiqx is a **portable, typed, and auditable AI-contract system**. Template rendering is one compiler stage inside that system.

One Templiqx AI contract describes one model interaction. Host code or an external orchestrator composes multiple interactions into workflows. Templiqx owns validation and provider-neutral compilation; model execution is supplied by optional adapters. It does not become a model gateway, RAG platform, or agent orchestrator.

## Requirements

### Portable AI contracts

- **R1. Canonical portable source.** A Templiqx contract must have one human-readable, diffable, declarative source representation that is independent of Rust and of any model provider. Rust macros and later language SDKs may provide typed authoring ergonomics, but must produce the same canonical semantics.
- **R2. One-interaction contract.** A contract must be able to declare instructions/messages, typed inputs, context slots, reusable components, structured output, model capability requirements, runtime-policy hints, provenance metadata, and evaluation fixtures for one model interaction. Immutable tool-contract references are part of the product direction but not required in the first POC slice.
- **R3. Typed validation.** Missing variables, incompatible types, invalid component composition, tool/schema mismatches, and impossible output contracts must be detected before model execution with actionable, source-addressed diagnostics.
- **R4. Safe deterministic expressions.** Portable templates may use typed interpolation, conditions, iteration, includes/components, and a bounded set of deterministic filters. They must not execute arbitrary Rust, shell, BeanShell, JavaScript, or provider code.
- **R5. Reproducible compilation.** Given the same package, inputs, target capabilities, and compiler version, Templiqx must produce the same canonical compiled interaction and content fingerprint.

### Provider-neutral compilation and optional execution

- **R6. Provider-neutral core.** The core must compile contracts without performing network calls or requiring provider credentials. Customer data, tenant policy, credentials, and model-gateway configuration remain host-owned runtime inputs.
- **R7. Capability negotiation.** Contracts must declare required model capabilities. Compilation must fail explicitly when a target cannot satisfy them; unsupported behavior must never be silently dropped.
- **R8. Controlled provider extensions.** Provider-specific options may be expressed only through typed, namespaced extensions that do not change the meaning of the portable core unnoticed.
- **R9. Optional execution adapters.** Runtime adapters may execute compiled interactions, including structured-output handling, streaming, and tool-call round trips. Templiqx ships a typed adapter port and deterministic fake/conformance adapter. Production adapters, including the Basenet model-gateway adapter, are host-owned and live outside the Templiqx core and package graph.
- **R10. Orchestration boundary.** Agent loops, workflow branching, durable workflow state, RAG retrieval, retry policy, tenant authorization, and model-routing policy remain outside the canonical Templiqx contract and core.

### Developer and agent parity

- **R11. Human-agent outcome parity.** Humans and agents must be able to achieve the same Templiqx outcomes against the same canonical application capabilities and shared artifacts. No product action is reserved solely because the actor is human or agent.
- **R12. Three first-class surfaces.** The Rust API, CLI, and MCP adapter must expose equivalent POC capabilities before the first public release. No surface may contain unique product semantics or hidden shortcuts.
- **R13. Atomic capabilities.** Humans and agents must be able to discover, inspect, create/update, validate, compile, render, migrate, test, diff, and explain contracts by composing atomic operations rather than invoking workflow-shaped tools. Publication and production promotion are host lifecycle concerns, not POC capabilities.
- **R14. Machine-readable diagnostics.** Every validation, compilation, migration, render, and evaluation result must have stable structured output suitable for agents and CI, alongside concise human-readable output.
- **R15. Actor-neutral capability model.** Templiqx itself must not hide product capabilities based on whether the actor is a human or an agent. Authentication, authorization, signing, publication, approval, and production activation are host-owned policy decisions. A host may require human acceptance for a legal proposal, as Basenet does, without creating a separate Templiqx API or execution path.

### Versioned, self-verifying packages

- **R16. Portable packages.** Contracts and components must be distributable as standalone semantically versioned packages through Git or ordinary artifact storage without requiring a central online registry.
- **R17. Package contents.** A publishable package must carry its contracts, typed schemas, components, immutable tool-contract references when used, fixtures/evals, compatibility migrations, manifest, dependency pins/lock data, checksums, and provenance.
- **R18. Verifiable identity.** Package and compiled-artifact identities must be content-addressable. The product direction includes signatures; the POC must at minimum prove deterministic hashes and verification, with signing allowed to follow after the core format is validated.
- **R19. No tenant payloads in packages.** Reusable packages must not contain production customer data, provider credentials, or tenant-specific authorization decisions.

### Legacy document compatibility

- **R20. Official compatibility module.** Legacy document support must be an optional adapter/module outside the AI-contract compiler core, while remaining a supported Templiqx capability across Rust, CLI, and MCP.
- **R21. Import and normalization.** The compatibility architecture must allow supported legacy V1/V2/V5 field references, Word merge fields, and related DOCX/ODT constructs to import into the canonical typed slot/component model with the source dialect recorded explicitly. Compatibility claims are always scoped to an enumerated dialect subset and fixture corpus.
- **R22. Unsafe legacy behavior is never executed.** V1 BeanShell and other arbitrary legacy code must be converted into declarative constructs where safely possible or returned as an explicit remediation item. Templiqx must never execute it.
- **R23. Explicit migration reporting.** Imports must produce a structured compatibility report distinguishing migrated, approximated, unsupported, and unsafe constructs. No compatibility loss may be silent.
- **R24. POC compatibility claim.** The first POC supports one V5 DOCX fixture class: `$data.*` references, ordinary Word merge fields, document body text, tables, headers/footers, unresolved-field reporting, and renamed-field aliases. V1/V2 and unsupported V5 constructs are detected and reported but not migrated in the POC. Parity means that, after removing volatile package metadata/ids and canonicalizing XML, populated merge values plus the body/table/header/footer structure match the approved baseline. Pixel-perfect rendering and ODT parity are not part of the POC claim.
- **R25. No runtime content sniffing.** Legacy dialect is identified during import and persisted as provenance; normal rendering never guesses an engine from template content.

### Basenet CRM3 conformance pilot

- **R26. Typed extraction scenario.** The POC must run one Basenet legal-document date/term extraction contract aligned with BLI-61 that produces schema-valid, source-linked proposed facts and deterministic validation results.
- **R27. Drafting scenario.** The POC must feed validated extracted facts plus host-supplied matter context into a separate drafting contract aligned with BLI-62 and produce a structured draft proposal. Acceptance or rejection of that proposal remains a Basenet host action.
- **R28. Document scenario.** The structured draft must render through at least one migrated Basenet DOCX template and pass normalized-OOXML comparison against its approved baseline.
- **R29. End-to-end traceability.** The resulting receipt must connect package/version hashes, contract ids, input/context fingerprints, target capability profile, compiled artifact, adapter identity, structured output validation, evaluation results, migration report, and final document artifact without embedding sensitive payloads. A live adapter must preserve deterministic request/contract fingerprints but is not expected to produce deterministic model output.
- **R30. Surface conformance.** The same checked-in pilot fixtures must be runnable through Rust, CLI, and MCP. Compilation and deterministic-fake execution must yield identical canonical results and fingerprints; live-adapter output equality is explicitly not asserted.

## POC Scope

The architecture-killing hypothesis is: **can one portable typed contract model be operated equally by developers and agents, compile deterministically, drive typed extraction and drafting, and bridge a representative V5 DOCX template without acquiring Basenet or legacy-runtime coupling?**

The POC is the minimum vertical proof of that hypothesis, not the complete commercial platform. It must include:

- one canonical declarative contract format;
- typed inputs, context slots, local components, bounded expressions, and structured outputs;
- a provider-neutral compiled representation with capability validation;
- a typed runtime adapter port and deterministic fake adapter; no live model-gateway call is required for POC completion;
- equivalent Rust, CLI, and MCP capabilities for discover, inspect, create/update, validate, compile, deterministic render, migrate, test, diff, and explain;
- one self-contained local package with manifest, version, deterministic hash, fixtures, and eval assertions; dependency resolution, signatures, publication, and promotion are not implemented;
- the V5 DOCX subset and parity fixture defined by R24, including one old field alias and one unresolved-field diagnostic;
- the extraction-to-drafting-to-DOCX Basenet conformance flow.

Features outside that slice are retained as product direction but should not delay proving the architecture.

## Success Criteria

- A fresh developer or agent can discover the package, validate it, understand failures, compile it, run the deterministic tests, and reproduce the Basenet pilot using only documented Rust, CLI, or MCP capabilities over the same shared files.
- Invalid types, missing context, unsupported provider capabilities, unsafe legacy code, and structured-output failures are rejected explicitly before a package can pass its local conformance checks.
- The Rust, CLI, and MCP pilot runs produce identical canonical artifact identities for the same deterministic inputs.
- A capability-parity check demonstrates that humans and agents can perform the same POC operation set against the same application port; Basenet production acceptance remains outside Templiqx.
- The legal extraction result validates against its declared schema and retains source provenance.
- The drafting contract consumes only validated inputs and produces a schema-valid draft.
- At least one representative migrated Basenet DOCX template passes the agreed normalized-OOXML parity comparison.
- The core contains no Basenet domain imports, model-provider SDK dependency, tenant secrets, workflow engine, or embedded legacy execution engine.

## Scope Boundaries

- No full agent orchestrator, durable workflow engine, planner, RAG implementation, embeddings store, or general-purpose tool executor.
- No ownership of Blinqx/Basenet model routing, credentials, tenant authorization, audit storage, or production approval service.
- No hosted registry, marketplace, visual editor, or multi-tenant control plane in the POC.
- No live provider/model-gateway call, streaming, tool-call round trip, package dependency resolver, signing, publication, promotion, or production activation in the POC.
- No promise to run arbitrary Handlebars, Jinja, Velocity, BeanShell, or Rust code unchanged.
- No full legacy-format coverage in the POC. Compatibility expands from measured fixture coverage, never from an unverified “supports everything” claim.
- No requirement for ODT parity in the first thin slice, although the compatibility contract must not preclude it.
- No provider-specific feature may enter the portable core merely to satisfy the Basenet pilot.
- Incremental/virtual-DOM rendering, zero-copy optimization, WASM packaging, and broad non-AI HTML/XML generation are deferred until the core contract and adoption model are validated.

## Key Decisions

- **Standalone product:** Multiple Blinqx companies must be able to adopt Templiqx without depending on Basenet CRM3.
- **AI-contract compiler, not another Handlebars:** Typed AI interaction contracts are the durable capability; text/document templating serves them.
- **Basenet is the first conformance host:** It supplies real extraction, drafting, gateway, and legacy-DOCX pressure without becoming core architecture.
- **Portable files are canonical:** This preserves cross-language adoption and makes contracts directly operable by agents.
- **Rust API, CLI, and MCP prove the same POC operations:** Developer and agent surfaces exercise one canonical application capability layer.
- **Execution is optional and adapter-owned:** The compiler stays deterministic and provider-neutral while real hosts can stream and execute.
- **One contract equals one interaction:** Workflows compose contracts outside Templiqx, avoiding overlap with the Blinqx orchestration platform.
- **Compatibility means safe migration plus measured render parity:** Legacy assets survive; unsafe legacy runtimes do not.
- **Packages carry contract plus evidence:** Tests, evals, provenance, migrations, and hashes travel with the artifact.
- **Humans and agents are capability peers inside Templiqx:** The same canonical operations and shared artifacts serve both. Host products retain their own authority and acceptance policies; Basenet may require human acceptance without changing Templiqx semantics.

## Alternatives Considered

### Rust-macro-first template engine

Strong Rust ergonomics and compile-time checks, but it would make Rust source the portability boundary and weaken direct agent editing and adoption by non-Rust Blinqx stacks.

### Compiler plus owned model gateway/runtime

Convenient as an all-in-one tool, but it would duplicate existing Blinqx model-gateway ownership and couple the template system to secrets, routing, retries, and provider operations.

### Unified template and agent-workflow platform

Potentially powerful, but it overlaps the existing agent-runtime/orchestration scope and turns a reusable contract engine into a high-carrying-cost platform.

### Embedded legacy engines

Offers superficial compatibility but retains arbitrary-code execution, Java-era dependencies, content-sniffing behavior, and unbounded semantic debt. Safe import with explicit gaps is the chosen alternative.

## Dependencies / Assumptions

- Basenet can provide a small, sanitized fixture corpus: representative source documents, expected extractions, one drafting case, a legacy DOCX template, fixture merge data, and an approved legacy-rendered baseline.
- The Basenet host owns authorization, data retrieval, redaction/pseudonymization, model-gateway configuration, and final approval/audit persistence.
- Existing Blinqx legacy-template research and normalized-reference decisions are valid inputs, but Templiqx remains free to define its own standalone adapter boundary.
- Provider and tool contracts are referenced immutably; actual tool execution and permission enforcement remain host responsibilities.

## Post-POC Validation Hypotheses

- A second Blinqx company can consume the package format without adopting Basenet-specific runtime code.
- A host-owned Basenet model-gateway adapter can execute the same compiled request while preserving request fingerprints and traceability.
- Streaming, tool contracts/round trips, dependency resolution, signing, publication/promotion, ODT, V1/V2 migration, WASM, and additional provider capability profiles can be added through the established ports without changing canonical POC semantics.

## Outstanding Questions

### Resolve Before Planning

None. The remaining choices are technical validation work, and the user has delegated POC-level decisions to the implementation team.

### Deferred to Planning

- **[Affects R1-R5][Needs research]** Select the canonical source syntax and compiled representation by prototyping for readability, lossless round-tripping, source diagnostics, schema evolution, and cross-language code generation.
- **[Affects R9][Technical]** Define the adapter port; the POC proves it with a deterministic fake, while any live Basenet adapter remains host-owned and post-POC.
- **[Affects R11-R15][Technical]** Define the command/tool shapes for the fixed POC operation set without adding workflow-shaped capabilities or a Templiqx authorization subsystem.
- **[Affects R18][Needs research]** Choose the eventual signing and trust model; deterministic hashing is sufficient for the POC acceptance gate.
- **[Affects R21-R24][Needs research]** Select a sanitized fixture matching the fixed V5 DOCX subset in R24 and document the volatile OOXML nodes/attributes excluded by canonicalization.
- **[Affects R26-R30][User/host input]** Obtain the Basenet pilot fixtures and identify the host integration owner.

## Next Steps

→ `/prompts:ce-plan` for structured implementation planning.
