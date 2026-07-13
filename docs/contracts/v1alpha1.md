# Portable contract format `templiqx/v1alpha1`

The canonical source is strict, human-readable YAML. Unknown fields and unknown enum values fail parsing. `examples/packages/demo/contracts/greeting.yaml` is the executable reference example.

A contract defines exactly one model interaction:

- typed `inputs` and host-supplied `context`, each described by JSON Schema;
- ordered role messages;
- required target capabilities;
- a JSON Schema structured output contract;
- bounded runtime hints and namespaced extensions;
- local components, provenance, and deterministic eval fixtures.

## Structured content

Content is data, not executable source. Supported nodes are:

- `text`: literal text;
- `interpolate`: a typed expression with fixed `trim`, `lower`, `upper`, `json`, `format_date`, or `format_number` filters;
- `when`: deterministic conditional content;
- `for_each`: deterministic iteration over an array;
- `component`: a local component invocation with explicit values;
- `include`: splice a package-relative partial (`path`) of content nodes, optionally sourced from a dependency package (`from_dependency`).

The `include` node is expanded by the composition layer before validation and compilation — the portable core never reads files. The referenced partial is a YAML list of content nodes; it is spliced in place and may itself contain further includes. Includes are cycle-checked (`TQX_INCLUDE_CYCLE`), path-confined like every package artifact (traversal yields `TQX_INCLUDE_UNRESOLVED`), and a malformed partial fails with `TQX_INCLUDE_INVALID`. After expansion the content tree contains no include nodes, so all downstream diagnostics are the ordinary content diagnostics.

Expressions are limited to references, JSON literals, equality, boolean negation, conjunction, and disjunction. Shell, Rust, JavaScript, BeanShell, provider code, template-language directives, and dynamic filters cannot execute.

References begin with `inputs.`, `context.`, an iteration item, or an explicit component argument. Every path segment is checked against the declared nested object/array schema, including paths below a `for_each` item. Missing, unknown, or structurally impossible paths are diagnostics, never empty-string fallbacks.

Boolean operators and `when` require booleans; `for_each` requires an array. Interpolation accepts schema-known scalar values directly. Arrays, objects, and null require the `json` filter, while `trim`, `lower`, and `upper` require strings. The `format_date` filter reformats an ISO `YYYY-MM-DD` string and `format_number` groups a numeric value, both driven by `context.locale` (`nl*`, `de*`, `en-US*`, else ISO/plain); they read locale data only and execute no code. Non-conforming input fails validation. Input, context, component-parameter, extension, and output schemas use the deliberately bounded POC JSON Schema subset. Unsupported keywords and impossible bounds fail validation. The `date` and `date-time` formats are enforced when values and runtime outputs are validated.

## Typed components

New components declare typed parameters and content:

```yaml
components:
  salutation:
    parameters:
      recipient:
        schema: { type: string, minLength: 1 }
        required: true
      formal:
        schema: { type: boolean }
        required: false
    content:
      - kind: interpolate
        expression: { kind: ref, path: recipient }
        filters: [trim]
```

A `component` node's `with` map is validated against that definition. Missing required parameters, undeclared arguments, incompatible argument types, and invalid nested parameter paths fail before rendering.

For compatibility, the original component form—a YAML list of content nodes—still parses. Its referenced external arguments are treated conservatively as required strings. Missing, unknown, incompatible, nested, boolean, or collection usage that cannot satisfy that inference is rejected. Authors needing non-string or nested arguments must migrate the component to the typed form; the core does not guess or silently coerce legacy behavior.

## Capabilities and extensions

Compilation requires an explicit target capability profile. Every contract requirement must be present or compilation fails with `TQX_CAPABILITY_UNSUPPORTED`. Extensions use namespaced keys such as `openai.reasoning_effort`; a bare key is rejected. Each extension is a typed declaration rather than an arbitrary value:

```yaml
extensions:
  openai.reasoning_effort:
    capability: openai.reasoning
    schema:
      type: string
      enum: [low, medium, high]
    value: medium
```

The extension schema must be in the bounded subset and its `value` must validate against it. Its declared capability must be present in the target profile or compilation fails with `TQX_EXTENSION_UNSUPPORTED`; it is also propagated into the compiled interaction's required capabilities so a runtime adapter cannot bypass the gate. The validated value is preserved in the compiled interaction and is never silently interpreted by the portable core.

### Tool-contract references

A package manifest may declare a `tool_contracts` table of shared, immutable, content-addressed schemas. An extension references one instead of inlining the schema by setting its `schema` to a `$ref` with a pinned fingerprint:

```yaml
# templiqx.yaml
tool_contracts:
  search_customers:
    fingerprint: sha256:abc...
    schema: { type: object, properties: { query: { type: string } }, required: [query] }
```

```yaml
# a contract's extension
extensions:
  vendor.search:
    capability: tools
    schema: { $ref: tool_contract:search_customers, fingerprint: sha256:abc... }
    value: { query: "acme" }
```

Resolution happens before validation: the reference is replaced with the referenced schema when the pinned fingerprint matches, so downstream validation and compilation see a fully-inlined bounded schema. An unknown name, a mismatched fingerprint, or a missing pin fails closed with `TQX_TOOL_CONTRACT_REF_UNRESOLVED`. Editing a shared schema yields a new fingerprint, so pinned references never resolve to a silently changed definition. The `tool_contracts` field is additive; manifests without it parse and fingerprint exactly as before.

## Stable diagnostics

All operations return `OperationEnvelope<T>` with `api_version`, `operation`, `ok`, optional `result`, `diagnostics`, and named `fingerprints`. Diagnostics carry a stable code and severity plus file, JSON pointer, source span, and help when available.
