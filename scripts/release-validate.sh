#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOW="$REPO_ROOT/.github/workflows/release.yml"

fail() {
  printf 'release validation: FAIL %s\n' "$*" >&2
  exit 1
}

command -v cargo >/dev/null 2>&1 || fail "cargo is required"
command -v ruby >/dev/null 2>&1 || fail "ruby is required for JSON/YAML validation"

workspace_version="$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' "$REPO_ROOT/Cargo.toml")"
version="${1:-$workspace_version}"
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]] ||
  fail "version must be SemVer without a leading v: $version"
[[ $version != *+* ]] ||
  fail "SemVer build metadata is not supported for release versions because it is not OCI-tag portable: $version"
[[ $workspace_version == "$version" ]] ||
  fail "workspace version $workspace_version does not match release version $version"

chart_version="$(sed -n 's/^version: *//p' "$REPO_ROOT/charts/templiqx/Chart.yaml")"
app_version="$(sed -n 's/^appVersion: *"\{0,1\}\([^" ]*\)"\{0,1\}$/\1/p' "$REPO_ROOT/charts/templiqx/Chart.yaml")"
[[ $chart_version == "$version" ]] ||
  fail "chart version $chart_version does not match release version $version"
[[ $app_version == "$version" ]] ||
  fail "chart appVersion $app_version does not match release version $version"

if [[ ${GITHUB_REF_TYPE:-} == tag ]]; then
  [[ ${GITHUB_REF_NAME:-} == "v$version" ]] ||
    fail "tag ${GITHUB_REF_NAME:-<missing>} must equal v$version"
fi

metadata="$(mktemp)"
trap 'rm -f "$metadata"' EXIT
(cd "$REPO_ROOT" && cargo metadata --format-version 1 --no-deps --locked >"$metadata")
ruby -rjson -e '
  metadata = JSON.parse(File.read(ARGV[0]))
  expected = ARGV[1]
  members = metadata.fetch("workspace_members")
  packages = metadata.fetch("packages").select { |package| members.include?(package.fetch("id")) }
  mismatches = packages.reject { |package| package.fetch("version") == expected }
  abort "workspace package version mismatch: #{mismatches.map { |p| "#{p["name"]}=#{p["version"]}" }.join(", ")}" unless mismatches.empty?
' "$metadata" "$version"

[[ -f $WORKFLOW ]] || fail "missing .github/workflows/release.yml"
for workflow in "$REPO_ROOT"/.github/workflows/*.yml; do
  ruby -e 'require "yaml"; YAML.parse_file(ARGV[0])' "$workflow"
done

for target in templiqx-cli templiqx-mcp templiqx-conformance; do
  grep -Fq "FROM gcr.io/distroless/static-debian13:nonroot@sha256:" "$REPO_ROOT/Dockerfile" ||
    fail "runtime base image is not digest-pinned"
  grep -Eq " AS ${target}$" "$REPO_ROOT/Dockerfile" ||
    fail "missing Docker target $target"
  grep -Fq "$target" "$WORKFLOW" || fail "release matrix omits $target"
done

grep -Fq 'linux/amd64,linux/arm64' "$WORKFLOW" || fail "release must build amd64 and arm64"
grep -Fq 'provenance: mode=max' "$WORKFLOW" || fail "BuildKit max provenance is required"
grep -Fq 'sbom: true' "$WORKFLOW" || fail "BuildKit SBOM attestation is required"
grep -Fq 'id-token: write' "$WORKFLOW" || fail "keyless signing requires OIDC permission"
grep -Fq 'cosign sign --yes' "$REPO_ROOT/scripts/release-verify-oci.sh" ||
  fail "digest signing step missing"
grep -Fq 'cosign verify' "$REPO_ROOT/scripts/release-verify-oci.sh" ||
  fail "digest signature verification step missing"
grep -Fq 'github.event_name == '\''push'\''' "$WORKFLOW" ||
  fail "publishing must be guarded by the tag push event"
# The GitHub expression is intentionally matched literally.
# shellcheck disable=SC2016
grep -Fq 'push: ${{ github.event_name == '\''push'\'' }}' "$WORKFLOW" ||
  fail "registry push must be disabled for workflow_dispatch"
grep -Fq 'gh release create' "$WORKFLOW" || fail "GitHub Release creation step missing"
grep -Fq 'sha256sum -c SHA256SUMS' "$WORKFLOW" || fail "release checksum readback missing"
[[ -x $REPO_ROOT/scripts/verify-packaged-chart.sh ]] ||
  fail "packaged chart verifier must exist and be executable"
grep -Fq './scripts/verify-packaged-chart.sh' "$WORKFLOW" ||
  fail "release must verify packaged chart defaults and install rendering"
grep -Fq 'release-templiqx-conformance-digest' "$WORKFLOW" ||
  fail "release chart must consume the verified conformance digest artifact"
grep -Fq 'values.fetch("image")["digest"] = digest' "$WORKFLOW" ||
  fail "release chart must pin the conformance image by digest"
grep -Fq 'include "templiqx.image"' "$REPO_ROOT/charts/templiqx/templates/conformance-job.yaml" ||
  fail "conformance Jobs must use the digest-aware image helper"
grep -Fq 'include "templiqx.image"' "$REPO_ROOT/charts/templiqx/templates/mock-gateway.yaml" ||
  fail "mock gateway must use the digest-aware image helper"
grep -Fq "prerelease == 'false'" "$WORKFLOW" ||
  fail "latest tags must be restricted to stable releases"

invalid_uses="$(awk '$1 == "-" && $2 == "uses:" { print $3 } $1 == "uses:" { print $2 }' "$REPO_ROOT"/.github/workflows/*.yml |
  grep -Ev '^[^@]+@[0-9a-f]{40}$' || true)"
[[ -z $invalid_uses ]] || fail "all workflow actions must be pinned to full commits: $invalid_uses"

printf 'release validation: OK version=%s targets=3 platforms=linux/amd64,linux/arm64\n' "$version"
