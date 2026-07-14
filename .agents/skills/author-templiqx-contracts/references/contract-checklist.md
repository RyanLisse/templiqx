# Contract checklist

- `apiVersion: templiqx/v1alpha1`
- Exactly one provider-neutral model interaction
- Strict YAML; no unknown fields or enum values
- Typed input and context JSON Schemas from the supported bounded subset
- Ordered role messages containing supported content nodes only
- References rooted at inputs, context, iteration items, or component arguments
- `json` filter for arrays, objects, and null
- Typed components for non-string or nested parameters
- Explicit required capabilities and explicit compile target profile
- Namespaced extensions with capability, schema, and value
- Structured output schema
- Package-relative, cycle-free includes
- Deterministic eval fixtures when behavior needs regression coverage

Canonical grammar: `docs/contracts/v1alpha1.md`. Executable example: `examples/packages/demo/contracts/greeting.yaml`.
