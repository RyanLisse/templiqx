# Templiqx Synthetic Conformance Chart

This is a **conformance-only** chart. It runs restricted jobs against the
synthetic Templiqx mock gateway using the `templiqx-conformance` image. It is
not a production Templiqx service chart and must not be used to deploy the CLI
or MCP product images.

The chart exposes only the in-cluster mock gateway required by the jobs. It
does not expose MCP or a general Templiqx HTTP API. Production host concerns
such as auth, tenancy, approval, retrieval, secrets, and provider wiring are
deliberately outside this chart.

Source-chart development remains backward-compatible with
`image.repository` plus `image.tag`. When `image.digest` is set, it takes
precedence and both the gateway and conformance Jobs render the immutable
`repository@sha256:...` reference. Published release charts set this digest to
the verified conformance OCI index.
