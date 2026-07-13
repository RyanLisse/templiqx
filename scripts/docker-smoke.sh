#!/usr/bin/env bash
set -euo pipefail

IMAGE="${IMAGE:-templiqx:pre-crm3}"
MCP_IMAGE="${MCP_IMAGE:-${IMAGE}-mcp}"
CONFORMANCE_IMAGE="${CONFORMANCE_IMAGE:-templiqx-conformance:pre-crm3}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="$REPO_ROOT/artifacts/docker-smoke"
LOCAL_WORKSPACE="$ARTIFACT_DIR/local-workspace"
CONTAINER_WORKSPACE="$(mktemp -d "${TMPDIR:-/tmp}/templiqx-docker-smoke-XXXXXX")"
LOCAL_RECEIPT="$ARTIFACT_DIR/local-receipt.json"
CONTAINER_RECEIPT="$CONTAINER_WORKSPACE/receipt.json"
HTTP_GOLDEN="$REPO_ROOT/scripts/golden/http-conformance.json"
HTTP_RECEIPT="$ARTIFACT_DIR/http-receipt.json"
INVENTORY="$REPO_ROOT/examples/crm3/scenarios/inventory.json"

cleanup() {
  # The hardened container writes into this bind mount as UID 65532, which has
  # no host-side entry; files it creates below the top-level workspace dir
  # aren't necessarily host-writable, so this cleanup is best-effort and must
  # never fail the run over an orphaned temp directory the runner discards anyway.
  rm -rf "$CONTAINER_WORKSPACE" 2>/dev/null || true
}
trap cleanup EXIT

skip_env() {
  if [[ ${CI:-} == "true" ]]; then
    printf 'FAIL command=./scripts/docker-smoke.sh reason=%s missing=%s\n' "$1" "$2" >&2
    exit 1
  fi
  printf 'SKIP_ENV command=./scripts/docker-smoke.sh reason=%s missing=%s\n' "$1" "$2"
  exit 0
}

command -v docker >/dev/null 2>&1 || skip_env "missing Docker CLI" "docker"
command -v jq >/dev/null 2>&1 || skip_env "missing jq" "jq"
docker info >/dev/null 2>&1 || skip_env "Docker daemon unavailable" "docker-daemon"
docker compose version >/dev/null 2>&1 || skip_env "missing Docker Compose plugin" "docker-compose"

resolve_docker_platform() {
  if [[ -n ${TEMPLIQX_DOCKER_PLATFORM:-} ]]; then
    printf '%s\n' "$TEMPLIQX_DOCKER_PLATFORM"
    return
  fi

  local docker_arch
  docker_arch="$(docker info --format '{{.Architecture}}')"
  case "$docker_arch" in
  amd64 | x86_64)
    printf 'linux/amd64\n'
    ;;
  arm64 | aarch64)
    printf 'linux/arm64\n'
    ;;
  *)
    printf 'FAIL command=./scripts/docker-smoke.sh reason=unsupported Docker server architecture arch=%s override=TEMPLIQX_DOCKER_PLATFORM\n' "$docker_arch" >&2
    exit 1
    ;;
  esac
}

DOCKER_PLATFORM="$(resolve_docker_platform)"
printf 'docker smoke: docker_platform=%s\n' "$DOCKER_PLATFORM"

rm -rf "$ARTIFACT_DIR"
mkdir -p "$LOCAL_WORKSPACE"
chmod 0777 "$CONTAINER_WORKSPACE"

cargo run -q -p templiqx-cli -- \
  --root "$REPO_ROOT/examples" \
  --json \
  crm3-conformance \
  --workspace "$LOCAL_WORKSPACE" \
  --receipt "$LOCAL_RECEIPT" >/dev/null

docker buildx build --load --platform "$DOCKER_PLATFORM" --target templiqx-cli -t "$IMAGE" "$REPO_ROOT"
docker buildx build --load --platform "$DOCKER_PLATFORM" --target templiqx-mcp -t "$MCP_IMAGE" "$REPO_ROOT"
docker buildx build --load --platform "$DOCKER_PLATFORM" --target templiqx-conformance -t "$CONFORMANCE_IMAGE" "$REPO_ROOT"

assert_image_paths() {
  local image="$1"
  shift
  local container archive
  container="$(docker create "$image")"
  archive="$ARTIFACT_DIR/${container}.tar"
  docker export "$container" >"$archive"
  docker rm "$container" >/dev/null
  for path in "$@"; do
    if tar -tf "$archive" | grep -Fxq "${path#/}"; then
      printf 'FAIL command=./scripts/docker-smoke.sh reason=forbidden-image-path image=%s path=%s\n' "$image" "$path" >&2
      exit 1
    fi
  done
  rm -f "$archive"
}

assert_image_paths "$IMAGE" \
  /usr/local/bin/templiqx-mcp \
  /usr/local/bin/templiqx-mock-gateway \
  /usr/local/bin/templiqx-http-conformance \
  /packages
assert_image_paths "$MCP_IMAGE" \
  /usr/local/bin/templiqx \
  /usr/local/bin/templiqx-mock-gateway \
  /usr/local/bin/templiqx-http-conformance \
  /packages

# Exercise the same HTTP adapter path used by the Helm job, not only the
# local deterministic service path.
CONFORMANCE_IMAGE="$CONFORMANCE_IMAGE" docker compose -f "$REPO_ROOT/deploy/compose.yml" --profile mock up -d mock-gateway
for _ in {1..30}; do
  curl -fsS "http://127.0.0.1:${TEMPLIQX_MOCK_GATEWAY_PORT:-18080}/health/ready" >/dev/null && break
  sleep 1
done
curl -fsS "http://127.0.0.1:${TEMPLIQX_MOCK_GATEWAY_PORT:-18080}/health/ready" >/dev/null
scenarios=()
while IFS= read -r scenario; do
  scenarios+=("$scenario")
done < <(jq -r '.scenarios[].id' "$INVENTORY")
[[ ${#scenarios[@]} -eq 8 ]] || {
  printf 'FAIL expected 8 inventory scenarios, got %s\n' "${#scenarios[@]}" >&2
  exit 1
}
for scenario in "${scenarios[@]}"; do
  receipt="$ARTIFACT_DIR/http-${scenario}.json"
  set +e
  CONFORMANCE_IMAGE="$CONFORMANCE_IMAGE" docker compose -f "$REPO_ROOT/deploy/compose.yml" --profile mock run --rm --no-deps \
    -e "TEMPLIQX_RUNTIME_SCENARIO=$scenario" conformance | tee "$receipt"
  compose_status=${PIPESTATUS[0]}
  set -e
  ((compose_status == 0)) || exit "$compose_status"
  jq -e --arg scenario "$scenario" '.ok == true and .scenario_id == $scenario' "$receipt" >/dev/null
done
cp "$ARTIFACT_DIR/http-intake-document-01.json" "$HTTP_RECEIPT"
jq -S . "$HTTP_RECEIPT" >/tmp/templiqx-http-receipt.sorted
jq -S . "$HTTP_GOLDEN" >/tmp/templiqx-http-golden.sorted
cmp /tmp/templiqx-http-receipt.sorted /tmp/templiqx-http-golden.sorted
CONFORMANCE_IMAGE="$CONFORMANCE_IMAGE" docker compose -f "$REPO_ROOT/deploy/compose.yml" --profile mock down --volumes

docker run --rm \
  --read-only \
  --user 65532:65532 \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --mount "type=bind,src=$REPO_ROOT/examples,dst=/packages,readonly" \
  --mount "type=bind,src=$CONTAINER_WORKSPACE,dst=/workspace" \
  "$IMAGE" \
  --root /packages \
  --json \
  crm3-conformance \
  --workspace /workspace \
  --receipt /workspace/receipt.json >/dev/null

cmp "$LOCAL_RECEIPT" "$CONTAINER_RECEIPT"
test -s "$CONTAINER_WORKSPACE/crm3/crm3-conformance/rendered.docx"
test ! -e "$REPO_ROOT/examples/crm3/crm3-conformance/rendered.docx"

# MCP transport is stdio-only: send a real initialize handshake and check the
# hardened container returns a matching JSON-RPC response before EOF closes it.
MCP_INITIALIZE='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"docker-smoke","version":"0.0.0"}}}'
MCP_RESPONSE="$(printf '%s\n' "$MCP_INITIALIZE" | docker run --rm -i \
  --read-only \
  --user 65532:65532 \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --mount "type=bind,src=$REPO_ROOT/examples,dst=/packages,readonly" \
  --mount "type=bind,src=$CONTAINER_WORKSPACE,dst=/workspace" \
  "$MCP_IMAGE" \
  /packages /workspace)"
printf '%s\n' "$MCP_RESPONSE" | jq -e '.result.serverInfo.name == "templiqx-mcp"' >/dev/null

printf 'docker smoke: mcp_stdio_initialize=ok\n'

MCP_SLIM_RESPONSE="$(printf '%s\n' "$MCP_INITIALIZE" | docker run --rm -i \
  --read-only \
  --user 65532:65532 \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --mount "type=bind,src=$REPO_ROOT/examples,dst=/packages,readonly" \
  --mount "type=bind,src=$CONTAINER_WORKSPACE,dst=/workspace" \
  "$MCP_IMAGE" \
  /packages /workspace)"
printf '%s\n' "$MCP_SLIM_RESPONSE" | jq -e '.result.serverInfo.name == "templiqx-mcp"' >/dev/null
printf 'docker smoke: mcp_slim_image=ok\n'

run_failure_smoke() {
  local profile="$1"
  local service="$2"
  local expected_code="$3"
  local log_file="$ARTIFACT_DIR/${profile}.log"
  set +e
  CONFORMANCE_IMAGE="$CONFORMANCE_IMAGE" docker compose -f "$REPO_ROOT/deploy/compose.yml" --profile "$profile" run --rm --no-deps "$service" | tee "$log_file"
  local status=${PIPESTATUS[0]}
  set -e
  if ((status != 2)); then
    printf 'FAIL command=./scripts/docker-smoke.sh reason=%s profile=%s exit=%s\n' \
      "expected failure exit code 2" "$profile" "$status" >&2
    exit 1
  fi
  if ! grep -F "\"code\":\"${expected_code}\"" "$log_file" >/dev/null; then
    printf 'FAIL command=./scripts/docker-smoke.sh reason=missing failure code profile=%s expected=%s\n' \
      "$profile" "$expected_code" >&2
    exit 1
  fi
  printf 'docker smoke: failure_profile=%s code=%s\n' "$profile" "$expected_code"
}

run_failure_smoke mock-failure-unavailable conformance-unavailable TQX_RUNTIME_UNAVAILABLE
run_failure_smoke mock-failure-timeout conformance-timeout TQX_RUNTIME_TIMEOUT

printf 'docker smoke: receipt_match=true artifact=%s\n' "$CONTAINER_WORKSPACE/crm3/crm3-conformance/rendered.docx"
