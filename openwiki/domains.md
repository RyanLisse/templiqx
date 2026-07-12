# Domain and contract model

This repository's product domain is a portable AI interaction contract system with explicit compatibility fixtures. The main business concepts are the contract format, capability requirements, deterministic evaluation inputs/outputs, and the CRM3 conformance package that proves a staged document workflow.

## Portable contract format

The canonical contract format is `templiqx/v1alpha1` and is represented as strict YAML. Important properties:

- unknown fields and unknown enum values are rejected;
- contracts describe exactly one model interaction;
- inputs and host context are typed with JSON Schema;
- structured content is data, not executable source;
- the output contract is JSON Schema-based;
- capabilities and namespaced extensions are explicit and validated;
- deterministic evaluation fixtures and provenance are part of the package.

Source: `docs/contracts/v1alpha1.md`.

The format is intentionally conservative. It supports only bounded content nodes such as `text`, `interpolate`, `when`, `for_each`, and `component`. Expressions are limited to references, JSON literals, equality, boolean logic, and a small filter set. This is what makes the core compiler deterministic and portable.

## Capability profile enforcement

Compilation requires an explicit target capability profile. If the contract needs a capability that is not present in the profile, compilation fails. Extensions are not free-form; they are namespaced and tied to a declared capability.

This is important because it keeps product behavior portable across hosts and prevents runtime adapters from silently bypassing required features.

## CRM3 conformance package

`examples/crm3` is a synthetic package used to prove a realistic multi-step workflow without shipping customer data.

It demonstrates:

- BLI-61 date-term extraction;
- BLI-62 document drafting from schema-valid extraction output;
- deterministic evaluation with checked-in request/output fixtures;
- migration from legacy DOCX V5 templates;
- rendering against a compatibility subset of the V5 fixture.

Source: `examples/crm3/README.md` and the checked-in scenario manifests under `examples/crm3/scenarios/**`.

The important product claim is narrow: the fixture proves an explicit compatibility subset, not arbitrary DOCX support.

## Mock scenarios and evidence discipline

The checked-in CRM3 scenarios are synthetic and data-driven through `templiqx.mock/v1alpha1`. The conformance tests verify grounded evidence, so the draft output cannot invent facts that were not traced back to the source fragment.

This is a core product boundary: the repository is not just generating text; it is preserving evidence traceability across interactions.

## DOCX V5 compatibility

`adapters/templiqx-docx-v5` and the CRM3 fixtures model a specific legacy-document migration path. The tests cover body paragraphs, table cells, header/footer parts, split-run alias migration, MERGEFIELD behavior, repeated references, and intentionally unresolved references.

That compatibility is chosen and documented. It should not be generalized without new evidence and tests.

## When changing domain logic

Be careful about these failure modes:

- relaxing the contract parser/validator so unsupported structures sneak in;
- adding capability or extension semantics without updating the portable format docs;
- changing CRM3 fixtures without preserving the evidence-grounding checks;
- expanding DOCX V5 support beyond the fixture subset without adding explicit tests.
