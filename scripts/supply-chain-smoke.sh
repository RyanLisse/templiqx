#!/usr/bin/env bash
set -euo pipefail

IMAGE="${IMAGE:-templiqx:pre-crm3}"
EXPECTED_PLATFORM="${EXPECTED_PLATFORM:-}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="$REPO_ROOT/artifacts/supply-chain"
SBOM_FILE="$ARTIFACT_DIR/sbom.spdx.json"
GRYPE_JSON="$ARTIFACT_DIR/grype.json"
GRYPE_LOG="$ARTIFACT_DIR/grype.log"
IMAGE_METADATA="$ARTIFACT_DIR/image-metadata.json"
IMAGE_INSPECT="$ARTIFACT_DIR/image-inspect.json"
BUILD_METADATA="$ARTIFACT_DIR/build-metadata.json"
PROVENANCE_FILE="$ARTIFACT_DIR/provenance.json"
FAILURE_LOG="$ARTIFACT_DIR/failure.log"

mkdir -p "$ARTIFACT_DIR"
rm -f "$FAILURE_LOG"

exit_code=0
trap 'exit_code=$?; printf "FAIL command=./scripts/supply-chain-smoke.sh status=%s line=%s image=%s\n" "$exit_code" "$LINENO" "$IMAGE" >>"$FAILURE_LOG"; exit "$exit_code"' ERR

fail() {
  printf 'FAIL command=./scripts/supply-chain-smoke.sh reason=%s %s\n' "$1" "${2:-}" | tee -a "$FAILURE_LOG" >&2
  exit 1
}

skip_env() {
  if [[ ${CI:-} == "true" ]]; then
    fail "$1" "missing=$2"
  fi
  printf 'SKIP_ENV command=./scripts/supply-chain-smoke.sh reason=%s missing=%s\n' "$1" "$2"
  exit 0
}

command -v docker >/dev/null 2>&1 || skip_env "missing Docker CLI" "docker"
docker info >/dev/null 2>&1 || skip_env "Docker daemon unavailable" "docker-daemon"
command -v syft >/dev/null 2>&1 || skip_env "missing SBOM scanner" "syft"
command -v grype >/dev/null 2>&1 || skip_env "missing vulnerability scanner" "grype"
command -v jq >/dev/null 2>&1 || skip_env "missing jq" "jq"

docker image inspect "$IMAGE" >"$IMAGE_INSPECT"

image_id="$(docker image inspect "$IMAGE" --format '{{.Id}}')"
image_os="$(docker image inspect "$IMAGE" --format '{{.Os}}')"
image_arch="$(docker image inspect "$IMAGE" --format '{{.Architecture}}')"
actual_platform="$image_os/$image_arch"

if [[ -z $image_id || $image_id != sha256:* ]]; then
  fail "missing-image-id" "image=$IMAGE"
fi

if [[ -n $EXPECTED_PLATFORM && $actual_platform != "$EXPECTED_PLATFORM" ]]; then
  fail "unexpected-platform" "expected=$EXPECTED_PLATFORM actual=$actual_platform image=$IMAGE"
fi

repo_digests="$(docker image inspect "$IMAGE" --format '{{json .RepoDigests}}')"
repo_tags="$(docker image inspect "$IMAGE" --format '{{json .RepoTags}}')"
created="$(docker image inspect "$IMAGE" --format '{{.Created}}')"
cat >"$IMAGE_METADATA" <<JSON
{
  "image": "$IMAGE",
  "image_id": "$image_id",
  "platform": "$actual_platform",
  "expected_platform": "$EXPECTED_PLATFORM",
  "repo_tags": $repo_tags,
  "repo_digests": $repo_digests,
  "created": "$created",
  "sbom": "artifacts/supply-chain/sbom.spdx.json",
  "vulnerability_scan": "artifacts/supply-chain/grype.json",
  "fail_on": "high"
}
JSON

syft packages "$IMAGE" --output "spdx-json=$SBOM_FILE"
test -s "$SBOM_FILE"

grype "$IMAGE" --output json >"$GRYPE_JSON"
test -s "$GRYPE_JSON"
grype "$IMAGE" --fail-on high --output table 2>&1 | tee "$GRYPE_LOG"

if [[ ${CI:-} == "true" ]]; then
  if [[ ! -s $BUILD_METADATA ]]; then
    fail "missing-build-metadata" "expected=$BUILD_METADATA"
  fi
  build_digest="$(jq -er '."containerimage.digest"' "$BUILD_METADATA")"
  image_id_short="${image_id#sha256:}"
  build_digest_short="${build_digest#sha256:}"
  if [[ $build_digest_short != "$image_id_short" ]]; then
    fail "provenance-digest-mismatch" "build=$build_digest inspect=$image_id"
  fi
  jq -n \
    --arg image "$IMAGE" \
    --arg digest "$image_id" \
    --arg platform "$actual_platform" \
    --arg sbom "$SBOM_FILE" \
    --slurpfile build "$BUILD_METADATA" \
    '{
      api_version: "templiqx/supply-chain/v1",
      image: $image,
      digest: $digest,
      platform: $platform,
      sbom: $sbom,
      build_metadata: $build[0]
    }' >"$PROVENANCE_FILE"
  test -s "$PROVENANCE_FILE"
else
  if [[ -s $BUILD_METADATA ]]; then
    printf 'supply chain smoke: build_metadata=%s\n' "$BUILD_METADATA"
  else
    printf 'SKIP_ENV supply chain smoke: build provenance metadata optional outside CI\n'
  fi
fi

if command -v cosign >/dev/null 2>&1 && [[ -n ${COSIGN_VERIFY:-} ]]; then
  cosign verify "$IMAGE" --certificate-identity-regexp='.*' --certificate-oidc-issuer-regexp='.*' \
    >/dev/null 2>&1 || skip_env "cosign verify unavailable for local tag" "cosign-verify"
fi

printf 'supply chain smoke: image=%s image_id=%s platform=%s sbom=%s scan=%s metadata=%s\n' \
  "$IMAGE" "$image_id" "$actual_platform" "$SBOM_FILE" "$GRYPE_JSON" "$IMAGE_METADATA"
