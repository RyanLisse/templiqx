# ADR: Tool-contract references

## Status

Accepted (2026-07-12) — design only; implementation deferred post-P2.

## Context

`templiqx/v1alpha1` compiles one interaction per contract. Provider tool/function
definitions currently only reach a contract through `ExtensionSpec` — a single
provider-specific value validated against a capability-guarded schema, inlined
in the authoring package (`Contract.extensions`). Every contract that wants the
same tool definition (e.g. a shared `search_customers` function schema used by
ten interactions) must duplicate that JSON inline.

R2 and R17 (brainstorm scope) name immutable tool-contract references as
product direction, explicitly deferred from the POC slice. This ADR specifies
the additive shape so a future package format can carry shared, versioned tool
definitions without changing how the current POC compiles or executes.

## Decision

1. **New optional package-level table, not a new node type.**
   A package manifest may declare a `tool_contracts` map of immutable,
   content-addressed definitions:

   ```yaml
   # templiqx.yaml (PackageManifest — additive field)
   tool_contracts:
     search_customers:
       fingerprint: sha256:9f2a...
       schema:
         type: object
         properties:
           query: { type: string }
         required: [query]
   ```

   `PackageManifest` gains `#[serde(default)] pub tool_contracts: BTreeMap<String, ToolContractRef>`
   alongside the existing `contracts`, `components`, `evals` lists. Absent the
   field, manifests parse exactly as today (`deny_unknown_fields` stays safe
   because this is additive, not a rename).

2. **Contracts reference by name + fingerprint, never inline the schema twice.**
   `Contract.extensions: BTreeMap<String, ExtensionSpec>` already carries a
   `capability` string and a `schema`/`value` pair per entry. A tool-contract
   reference reuses that shape: `ExtensionSpec.schema` becomes
   `{ "$ref": "tool_contract:search_customers", "fingerprint": "sha256:9f2a..." }`
   instead of the inlined JSON Schema. Compilation resolves the `$ref` against
   the package's `tool_contracts` table and fails closed
   (`TQX_TOOL_CONTRACT_REF_UNRESOLVED`) if the name or fingerprint doesn't
   match — the same fail-closed posture package signing (`adr-package-trust.md`)
   already uses for unverifiable input.

3. **Immutability via fingerprint, not a version number.**
   Consistent with `PackageManifest.contracts`/`components` (name-addressed,
   content-hashed via `canonical_json`), a tool-contract entry's identity is
   its `fingerprint`. Editing the schema produces a new fingerprint; existing
   `$ref`s pinned to the old fingerprint keep resolving to the old definition
   until authors bump the reference. No SemVer resolver, no dependency graph —
   matches the "package dependency locks" framing in R11 without introducing a
   registry.

4. **No runtime behavior change.** `ExecutionRequest`/`ExecutionReceipt` and
   the `RuntimeAdapter` trait are untouched. Resolution happens entirely in
   `templiqx-core` at compile time; `CompiledInteraction.extensions` still
   carries a fully-resolved schema by the time it reaches an adapter, so
   `templiqx-mock` and any production adapter keep working unmodified.

## Consequences

- Authors can share one tool schema across many interactions and packages
  without inlining it, and rotate a tool schema deliberately (new
  fingerprint) rather than accidentally (edit in place breaking every
  consumer silently).
- Compile-time resolution keeps the portable core boundary intact — no
  network fetch, no external registry, no host-owned vocabulary in
  `templiqx-contracts`/`templiqx-core`.
- Package inventory/signing (`PackageManifest.signatures`) naturally covers
  `tool_contracts` once it's a manifest field — no separate signing path
  needed.

## Alternatives considered

- **Inline-only, no dedup (status quo).** Rejected — the duplication R17
  flags as a growing pain in the brainstorm becomes worse as tool schemas
  used across an org's package set diverge silently.
- **External registry with live resolution.** Rejected for this slice —
  Templiqx core stays offline/deterministic; a registry is a host concern
  layered on top, not core to the contract format.
- **SemVer ranges on tool-contract references.** Rejected — fingerprint
  pinning is simpler, matches existing package/component identity, and
  avoids building a dependency resolver inside a "no policy" DTO crate.

## Open questions

- Whether `tool_contracts` entries can themselves reference other
  `tool_contracts` entries (composition) — deferred; no POC use case yet.
- Whether cross-package references (not just intra-package) are needed before
  this is implemented — tracked as a P2+ open question, not blocking this ADR.
