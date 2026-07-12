# Pre-CRM3 Readiness

Templiqx keeps CRM3-shaped proof work in conformance fixtures and deployment smoke paths. Core crates remain provider-neutral and CRM3-neutral.

## Workspace Contract

Packages are read-only inputs. Runtime artifacts are written to a separate workspace:

```sh
templiqx --root examples render-document crm3 templates/v5-contract-template.docx merge-data.json rendered.docx --workspace /tmp/templiqx-workspace
```

If `--workspace` is omitted, local composition uses `.templiqx-workspace` under the package collection root. Artifact references returned by Rust, CLI, and MCP remain portable paths relative to the selected workspace package directory.

## Failure Semantics

Runtime adapters report typed failures with stable diagnostic codes:

- `TQX_RUNTIME_TIMEOUT`
- `TQX_RUNTIME_RATE_LIMITED`
- `TQX_RUNTIME_UNAVAILABLE`
- `TQX_RUNTIME_INVALID_RESPONSE`
- `TQX_RUNTIME_PERMANENT`
- `TQX_HOST_RETRY_EXHAUSTED`

Failure envelopes have `ok=false` and no successful `ExecutionReceipt`.

## Deploy Checks

Always-required local gates:

```sh
just verify
```

Environment-dependent deploy gates:

```sh
just verify-deploy
```

Compose failure profiles (`mock-failure-unavailable`, `mock-failure-timeout`) and the kind gateway-down job prove typed transport failures and host retry exhaustion without sleeps in test code. Golden fixture updates require a `GOLDEN_REVIEW:` commit marker or `ALLOW_GOLDEN_UPDATE=1` in CI.
