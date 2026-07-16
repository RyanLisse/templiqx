# Templiqx Deployment Chart

This chart can deploy the **local/demo** Operations HTTP server
(`templiqx-http-server`) with `httpServer.runtime.mode: deterministic-fake` by
default (no credentials). That mode is **not** production-ready host operation
and the HTTP server image is **not** an official signed release artifact — see
`docs/adr/http-server-release-artifact.md`. Set `httpServer.runtime.mode` to
`langfuse`, provide the model and Langfuse endpoints, and reference an existing
Kubernetes Secret for optional real-model wiring in non-release environments.
The chart always sets `TEMPLIQX_RUNTIME_MODE` from `httpServer.runtime.mode`.

The synthetic conformance jobs and mock gateway remain independently enabled
through `values-mock.yaml` and use the separate signed `templiqx-conformance`
image. That profile disables the demo HTTP server and does not expose MCP.

Source-chart development remains backward-compatible with
`image.repository` plus `image.tag`. When `image.digest` is set, it takes
precedence and both the gateway and conformance Jobs render the immutable
`repository@sha256:...` reference. Published release charts set this digest to
the verified conformance OCI index.
