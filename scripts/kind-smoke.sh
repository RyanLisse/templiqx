#!/usr/bin/env bash
set -euo pipefail

IMAGE="${IMAGE:-templiqx:pre-crm3}"
CLUSTER="${KIND_CLUSTER:-templiqx-smoke}"
RELEASE="${HELM_RELEASE:-templiqx}"
NAMESPACE="${NAMESPACE:-templiqx-smoke}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="$REPO_ROOT/artifacts/kind-smoke"
LOG_FILE="$ARTIFACT_DIR/conformance.log"
KIND_RECEIPT="$ARTIFACT_DIR/http-receipt.json"
HTTP_GOLDEN="$REPO_ROOT/scripts/golden/http-conformance.json"

skip_env() {
  if [[ "${CI:-}" == "true" ]]; then
    printf 'FAIL command=./scripts/kind-smoke.sh reason=%s missing=%s\n' "$1" "$2" >&2
    exit 1
  fi
  printf 'SKIP_ENV command=./scripts/kind-smoke.sh reason=%s missing=%s\n' "$1" "$2"
  exit 0
}

command -v helm >/dev/null 2>&1 || skip_env "missing Helm CLI" "helm"
command -v kubectl >/dev/null 2>&1 || skip_env "missing kubectl CLI" "kubectl"
command -v kind >/dev/null 2>&1 || skip_env "missing kind CLI" "kind"
command -v jq >/dev/null 2>&1 || skip_env "missing jq" "jq"
command -v docker >/dev/null 2>&1 || skip_env "missing Docker CLI" "docker"
docker info >/dev/null 2>&1 || skip_env "Docker daemon unavailable" "docker-daemon"

resolve_docker_platform() {
if [[ -n "${TEMPLIQX_DOCKER_PLATFORM:-}" ]]; then
printf '%s\n' "$TEMPLIQX_DOCKER_PLATFORM"
return
fi

local docker_arch
docker_arch="$(docker info --format '{{.Architecture}}')"
case "$docker_arch" in
amd64|x86_64)
printf 'linux/amd64\n'
;;
arm64|aarch64)
printf 'linux/arm64\n'
;;
*)
printf 'FAIL command=./scripts/kind-smoke.sh reason=unsupported Docker server architecture arch=%s override=TEMPLIQX_DOCKER_PLATFORM\n' "$docker_arch" >&2
exit 1
;;
esac
}

DOCKER_PLATFORM="$(resolve_docker_platform)"
printf 'kind smoke: docker_platform=%s\n' "$DOCKER_PLATFORM"

rm -rf "$ARTIFACT_DIR"
mkdir -p "$ARTIFACT_DIR"

docker buildx build --load --platform "$DOCKER_PLATFORM" --target templiqx-cli -t "$IMAGE" "$REPO_ROOT"

if ! kind get clusters | grep -Fxq "$CLUSTER"; then
  kind create cluster --name "$CLUSTER"
fi

kind load docker-image "$IMAGE" --name "$CLUSTER"
kubectl create namespace "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -
kubectl delete job "$RELEASE-templiqx-conformance" -n "$NAMESPACE" --ignore-not-found
helm upgrade --install "$RELEASE" "$REPO_ROOT/charts/templiqx" \
  --namespace "$NAMESPACE" \
  -f "$REPO_ROOT/charts/templiqx/values-mock.yaml" \
  --set image.pullPolicy=Never

kubectl -n "$NAMESPACE" rollout status "deployment/$RELEASE-templiqx-mock-gateway" --timeout=120s
kubectl -n "$NAMESPACE" wait --for=condition=complete "job/$RELEASE-templiqx-conformance" --timeout=180s
kubectl -n "$NAMESPACE" logs "job/$RELEASE-templiqx-conformance" | tee "$LOG_FILE"

grep '"api_version":"templiqx/http-conformance/v1"' "$LOG_FILE" | tail -n 1 >"$KIND_RECEIPT"
jq -S . "$KIND_RECEIPT" >/tmp/templiqx-kind-http-receipt.sorted
jq -S . "$HTTP_GOLDEN" >/tmp/templiqx-kind-http-golden.sorted
cmp /tmp/templiqx-kind-http-receipt.sorted /tmp/templiqx-kind-http-golden.sorted
printf 'kind smoke: http_conformance=true success=true log=%s\n' "$LOG_FILE"

kubectl -n "$NAMESPACE" scale "deployment/$RELEASE-templiqx-mock-gateway" --replicas=0
kubectl -n "$NAMESPACE" rollout status "deployment/$RELEASE-templiqx-mock-gateway" --timeout=120s
kubectl -n "$NAMESPACE" delete job "$RELEASE-templiqx-conformance-gateway-down" --ignore-not-found

GATEWAY_DOWN_LOG="$ARTIFACT_DIR/gateway-down.log"
cat <<EOF | kubectl apply -f -
apiVersion: batch/v1
kind: Job
metadata:
  name: ${RELEASE}-conformance-gateway-down
  namespace: ${NAMESPACE}
spec:
  backoffLimit: 0
  template:
    metadata:
      labels:
        app.kubernetes.io/name: templiqx
        app.kubernetes.io/component: conformance-gateway-down
    spec:
      restartPolicy: Never
      securityContext:
        runAsNonRoot: true
        seccompProfile:
          type: RuntimeDefault
      containers:
        - name: conformance
          image: ${IMAGE}
          imagePullPolicy: Never
          command: ["/usr/local/bin/templiqx-http-conformance"]
          env:
            - name: TEMPLIQX_RUNTIME_URL
              value: "http://${RELEASE}-templiqx-mock-gateway:8080"
            - name: TEMPLIQX_HTTP_CONFORMANCE_MAX_ATTEMPTS
              value: "3"
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            runAsNonRoot: true
            runAsUser: 65532
            capabilities:
              drop: ["ALL"]
EOF

set +e
kubectl -n "$NAMESPACE" wait --for=condition=failed "job/${RELEASE}-conformance-gateway-down" --timeout=180s
gateway_down_wait=$?
set -e
if (( gateway_down_wait == 0 )); then
  :
elif ! kubectl -n "$NAMESPACE" get "job/${RELEASE}-conformance-gateway-down" -o jsonpath='{.status.failed}' | grep -q '^[1-9]'; then
  printf 'FAIL command=./scripts/kind-smoke.sh reason=gateway-down job did not fail\n' >&2
  exit 1
fi
kubectl -n "$NAMESPACE" logs "job/${RELEASE}-conformance-gateway-down" | tee "$GATEWAY_DOWN_LOG"
if ! grep -F '"code":"TQX_HOST_RETRY_EXHAUSTED"' "$GATEWAY_DOWN_LOG" >/dev/null; then
  printf 'FAIL command=./scripts/kind-smoke.sh reason=gateway-down missing retry-exhausted diagnostic\n' >&2
  exit 1
fi
printf 'kind smoke: gateway_down=true code=TQX_HOST_RETRY_EXHAUSTED log=%s\n' "$GATEWAY_DOWN_LOG"
