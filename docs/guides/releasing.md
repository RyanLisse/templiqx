---
title: Releasing Templiqx
---

Templiqx releases three deliberately separate OCI artifacts and one Helm chart:

- `ghcr.io/ryanlisse/templiqx-cli` — the standalone compiler CLI;
- `ghcr.io/ryanlisse/templiqx-mcp` — the MCP stdio transport;
- `ghcr.io/ryanlisse/templiqx-conformance` — explicitly synthetic mock and conformance tooling;
- `templiqx-<version>.tgz` — the synthetic-conformance Helm chart.

### Explicit non-artifact: `templiqx-http-server`

**Decision (accepted):** the Operations HTTP binary / Docker target
`templiqx-http-server` is **not** an official signed release artifact. It exists
for local/demo Compose and chart smoke (`TEMPLIQX_RUNTIME_MODE=deterministic-fake`
by default). Tag release does not build, push, or Cosign-sign it. Production
hosts should compose adapters and bind `templiqx_http::router` themselves. See
[ADR: HTTP server release artifact](../adr/http-server-release-artifact.md).

The conformance image and chart are not production services. A release proves
Templiqx-owned compiler, packaging, and synthetic conformance readiness. CRM3
host wiring, tenant policy, production data, and opco acceptance remain outside
this repository.

## Version contract

The workspace version in `Cargo.toml`, every workspace package, and both
`version` and `appVersion` in `charts/templiqx/Chart.yaml` must match. Validate
the release definition locally before creating a tag:

```sh
./scripts/release-validate.sh 0.1.0
```

Run the `release` workflow manually with that version first. A
`workflow_dispatch` is always a non-publishing dry run: it executes repository
gates, builds all three images for `linux/amd64` and `linux/arm64`, and packages
and checksums the chart. Registry login, image push, signing, and GitHub Release
creation are guarded by the tag-push event.

After the dry run and normal CI are green, create and push the exact annotated
tag:

```sh
git tag -a v0.1.0 -m "Templiqx 0.1.0"
git push origin v0.1.0
```

The workflow rejects a tag that differs from `v<workspace-version>`, generated
or lockfile drift, an incomplete platform index, and chart/version mismatch.
SemVer prereleases are supported, but build metadata is rejected because `+`
is not portable in OCI tags. Prereleases never update the `latest` image tags
and are marked as prereleases in GitHub.

For a tag release, the packaging job reads the built conformance image's
registry-resolved digest and rewrites the chart defaults to
`repository@sha256:...`; the version tag remains metadata and a fallback for
local source-chart use. It enables the synthetic gateway and checks the
resulting archive through `helm template` and client-side `helm install
--dry-run`. A released chart therefore runs all eight scenarios from the exact
signed image without a private values override; `values-mock.yaml` remains the
local-image override.

## Published evidence

For a validated tag, BuildKit pushes multi-platform indexes with registry-
attached SBOM and max-mode provenance attestations. The workflow resolves each
version tag back to the build digest, verifies both declared Linux platforms,
then keylessly signs and verifies that immutable digest with Cosign. The chart
is checksummed, keylessly signed as a blob, verified, and attached to the GitHub
Release with:

- the three immutable image references (`*.digest`);
- `release-manifest.json`;
- chart `SHA256SUMS` and complete `RELEASE-SHA256SUMS`;
- the chart Sigstore bundle.

Verify an image independently using the exact identity and digest from the
release manifest:

```sh
cosign verify \
  --certificate-identity "https://github.com/RyanLisse/templiqx/.github/workflows/release.yml@refs/tags/v0.1.0" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  "ghcr.io/ryanlisse/templiqx-cli@sha256:<digest>"
```

Verify downloaded release files from the release directory:

```sh
sha256sum -c RELEASE-SHA256SUMS
cosign verify-blob \
  --bundle templiqx-0.1.0.tgz.sigstore.json \
  --certificate-identity "https://github.com/RyanLisse/templiqx/.github/workflows/release.yml@refs/tags/v0.1.0" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  templiqx-0.1.0.tgz
```

Never verify or deploy by a mutable tag alone; use the recorded
`image@sha256:...` reference. Released chart archives already pin the
conformance workload to that digest through `image.digest`.
