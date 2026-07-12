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
  if [[ "${CI:-}" == "true" ]]; then
    fail "$1" "missing=$2"
  fi
  printf 'SKIP_ENV command=./scripts/supply-chain-smoke.sh reason=%s missing=%s\n' "$1" "$2"
  exit 0
}

command -v docker >/dev/null 2>&1 || skip_env "missing Docker CLI" "docker"
docker info >/dev/null 2>&1 || skip_env "Docker daemon unavailable" "docker-daemon"
command -v syft >/dev/null 2>&1 || skip_env "missing SBOM scanner" "syft"
command -v grype >/dev/null 2>&1 || skip_env "missing vulnerability scanner" "grype"

docker image inspect "$IMAGE" >"$IMAGE_INSPECT"

image_id="$(docker image inspect "$IMAGE" --format '{{.Id}}')"
image_os="$(docker image inspect "$IMAGE" --format '{{.Os}}')"
image_arch="$(docker image inspect "$IMAGE" --format '{{.Architecture}}')"
actual_platform="$image_os/$image_arch"

if [[ -z "$image_id" || "$image_id" != sha256:* ]]; then
  fail "missing-image-id" "image=$IMAGE"
fi

if [[ -n "$EXPECTED_PLATFORM" && "$actual_platform" != "$EXPECTED_PLATFORM" ]]; then
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

printf 'supply chain smoke: image=%s image_id=%s platform=%s sbom=%s scan=%s metadata=%s\n' \
  "$IMAGE" "$image_id" "$actual_platform" "$SBOM_FILE" "$GRYPE_JSON" "$IMAGE_METADATA"
