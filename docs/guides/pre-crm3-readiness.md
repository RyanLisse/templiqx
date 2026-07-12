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

Missing Docker daemon, Helm, kubectl, kind, Syft, Grype, or Trivy is reported as `SKIP_ENV` in local runs. In CI, configured deploy lanes should treat missing configured tools as failures.
