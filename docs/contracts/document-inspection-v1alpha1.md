# Document inspection v1alpha1

Read-only, non-mutating preflight for package-confined document templates.

## Operation

`inspect_document` resolves a package-relative template path, invokes the
configured `DocumentInspector` adapter for the requested dialect, and returns a
typed compatibility report without acquiring a workspace lease or writing an
artifact.

## Request

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `package` | string | yes | Portable package name |
| `dialect` | string | yes | Explicit adapter dialect (e.g. `v5`) |
| `template` | string | yes | Package-relative template path |
| `aliases` | object | no | Migration alias map for dialect analysis |

Unknown fields are rejected (`deny_unknown_fields`).

## Result

| Field | Type | Notes |
|-------|------|-------|
| `report` | object | Adapter-specific compatibility report |

For the DOCX V5 adapter the report matches `CompatibilityReport`:

- `dialect`, category counts (`migrated`, `approximated`, `unsupported`,
  `unsafe_constructs`, `unresolved`)
- `findings[]` with `category`, `part`, `construct`, optional `reference`,
  `detail`

## Diagnostics

Path confinement failures (`TQX_PATH_INVALID`), missing templates
(`TQX_NOT_FOUND`), unsupported dialects (`TQX_UNSUPPORTED`), malformed archives
(`TQX_INVALID_DATA`), and I/O errors propagate as stable operation diagnostics.
Unsupported document constructs are **report data**, not transport errors.

## Actor parity

Rust (`TempliqxService::inspect_document`), CLI (`inspect-document`), MCP
(`inspect_document`), and HTTP (`POST /operations/v1/documents/inspect`) share
the same request/result envelope.

## Boundaries

- Inspection does not substitute for rendering; it never writes workspace output.
- Format-specific analysis stays in adapters; the portable core does not probe
  host installations or execute template code.
- Only constructs backed by corpus fixtures may be called "supported" in
  adapter documentation.
