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
- `interpolate`: a typed expression with fixed `trim`, `lower`, `upper`, or `json` filters;
- `when`: deterministic conditional content;
- `for_each`: deterministic iteration over an array;
- `component`: a local component invocation with explicit values.

Expressions are limited to references, JSON literals, equality, boolean negation, conjunction, and disjunction. Shell, Rust, JavaScript, BeanShell, provider code, template-language directives, and dynamic filters cannot execute.

References begin with `inputs.`, `context.`, an iteration item, or an explicit component argument. Every path segment is checked against the declared nested object/array schema, including paths below a `for_each` item. Missing, unknown, or structurally impossible paths are diagnostics, never empty-string fallbacks.

Boolean operators and `when` require booleans; `for_each` requires an array. Interpolation accepts schema-known scalar values directly. Arrays, objects, and null require the `json` filter, while `trim`, `lower`, and `upper` require strings. Input, context, component-parameter, extension, and output schemas use the deliberately bounded POC JSON Schema subset. Unsupported keywords and impossible bounds fail validation. The `date` and `date-time` formats are enforced when values and runtime outputs are validated.

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

## Stable diagnostics

All operations return `OperationEnvelope<T>` with `api_version`, `operation`, `ok`, optional `result`, `diagnostics`, and named `fingerprints`. Diagnostics carry a stable code and severity plus file, JSON pointer, source span, and help when available.
