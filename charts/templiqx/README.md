# Templiqx Deployment Chart

This chart deploys the Templiqx Operations HTTP server in deterministic-fake
mode by default, with no credentials required. Set `httpServer.runtime.mode`
to `langfuse`, provide the model and Langfuse endpoints, and reference an
existing Kubernetes Secret to enable the host-owned real runtime.

The synthetic conformance jobs and mock gateway remain independently enabled
through `values-mock.yaml` and use the separate `templiqx-conformance` image.
That profile disables the product HTTP server and does not expose MCP.

Source-chart development remains backward-compatible with
`image.repository` plus `image.tag`. When `image.digest` is set, it takes
precedence and both the gateway and conformance Jobs render the immutable
`repository@sha256:...` reference. Published release charts set this digest to
the verified conformance OCI index.
