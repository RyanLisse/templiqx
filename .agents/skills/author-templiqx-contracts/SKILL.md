---
name: author-templiqx-contracts
description: Create, edit, explain, and repair strict templiqx/v1alpha1 YAML contracts and package-local partials. Use for Templiqx prompt contracts, typed inputs and context, components, deterministic expressions, structured output schemas, capabilities, extensions, includes, or validation diagnostics.
---

# Author Templiqx Contracts

Use Templiqx operations to write and validate contracts; do not edit package files behind the application when `put_contract` is available.

## Author safely

1. Inspect an existing contract or start from `examples/packages/demo/contracts/greeting.yaml`.
2. Define exactly one model interaction with typed `inputs`, typed `context`, ordered messages, explicit capability requirements, and a bounded output JSON Schema.
3. Use only supported content nodes: `text`, `interpolate`, `when`, `for_each`, `component`, and `include`.
4. Use typed components for non-string or nested parameters. Do not rely on coercion.
5. Keep expressions deterministic: references, JSON literals, equality, negation, conjunction, and disjunction only.
6. Namespace extensions and declare their required capability and bounded schema.
7. Submit with `put_contract`, then run `explain_contract`, `validate_contract`, and `compile_contract` with an explicit target capability profile.

Read [references/contract-checklist.md](references/contract-checklist.md) before creating a contract from scratch.

## Repair diagnostics

- For undefined components or unresolved references, call `explain_contract` and follow its graph and fix hints.
- For capability failures, change the target profile or the contract requirement explicitly; never silently drop a requirement.
- For schema failures, reduce schemas to the supported subset instead of bypassing validation.
- For include failures, keep partials package-relative and eliminate cycles.
- Preserve stable diagnostic codes and source spans in the report.

## Verify completion

Require an `ok` validation envelope and successful compile before claiming the contract is ready. If fixtures exist, invoke the package-testing workflow and report fingerprints.
