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
  if [[ ${CI:-} == "true" ]]; then
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
kubectl -n "$NAMESPACE" scale "deployment/$RELEASE-templiqx-mock-gateway" --replicas=1 2>/dev/null || true
# A prior run may have scaled mock-gateway to 0 (gateway-down scenario). The
# scale-up above only requests the change; without waiting for the new pod's
# readiness probe here, helm's post-upgrade hook Jobs below can start against
# a pod that isn't serving yet and exhaust their retries in ~20s.
if kubectl -n "$NAMESPACE" get "deployment/$RELEASE-templiqx-mock-gateway" >/dev/null 2>&1; then
  kubectl -n "$NAMESPACE" rollout status "deployment/$RELEASE-templiqx-mock-gateway" --timeout=120s
fi
kubectl delete job -l "app.kubernetes.io/name=templiqx,templiqx.conformance/scenario" -n "$NAMESPACE" --ignore-not-found
helm upgrade --install "$RELEASE" "$REPO_ROOT/charts/templiqx" \
  --namespace "$NAMESPACE" \
  -f "$REPO_ROOT/charts/templiqx/values-mock.yaml" \
  --set image.pullPolicy=Never \
  --wait --timeout 5m

kubectl -n "$NAMESPACE" rollout status "deployment/$RELEASE-templiqx-mock-gateway" --timeout=120s

SCENARIOS=(intake-document-01 draft-with-citations invalid-output-schema)
for scenario in "${SCENARIOS[@]}"; do
  job_name="${RELEASE}-templiqx-conformance-${scenario//./-}"
  for _ in {1..30}; do
    if kubectl -n "$NAMESPACE" get "job/$job_name" >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  set +e
  kubectl -n "$NAMESPACE" wait --for=condition=complete "job/$job_name" --timeout=180s
  scenario_wait=$?
  set -e
  if ((scenario_wait != 0)); then
    printf 'FAIL command=./scripts/kind-smoke.sh reason=conformance job did not complete scenario=%s\n' "$scenario" >&2
    kubectl -n "$NAMESPACE" get "job/$job_name" -o yaml >&2 || true
    kubectl -n "$NAMESPACE" logs "job/$job_name" --all-containers --tail=200 >&2 || true
    exit "$scenario_wait"
  fi
  printf 'kind smoke: scenario=%s success=true\n' "$scenario"
done

kubectl -n "$NAMESPACE" logs "job/${RELEASE}-templiqx-conformance-intake-document-01" | tee "$LOG_FILE"

grep '"api_version":"templiqx/http-conformance/v1"' "$LOG_FILE" | tail -n 1 >"$KIND_RECEIPT"
jq -S . "$KIND_RECEIPT" >/tmp/templiqx-kind-http-receipt.sorted
jq -S . "$HTTP_GOLDEN" >/tmp/templiqx-kind-http-golden.sorted
cmp /tmp/templiqx-kind-http-receipt.sorted /tmp/templiqx-kind-http-golden.sorted
printf 'kind smoke: http_conformance=true success=true log=%s\n' "$LOG_FILE"

kubectl -n "$NAMESPACE" scale "deployment/$RELEASE-templiqx-mock-gateway" --replicas=0
kubectl -n "$NAMESPACE" rollout status "deployment/$RELEASE-templiqx-mock-gateway" --timeout=120s
# Terminating pods can keep serving until endpoints drain; wait for none left.
for _ in {1..60}; do
  if ! kubectl -n "$NAMESPACE" get endpoints "$RELEASE-templiqx-mock-gateway" \
    -o jsonpath='{.subsets[*].addresses[*].ip}' 2>/dev/null | grep -q .; then
    break
  fi
  sleep 2
done
if kubectl -n "$NAMESPACE" get endpoints "$RELEASE-templiqx-mock-gateway" \
  -o jsonpath='{.subsets[*].addresses[*].ip}' 2>/dev/null | grep -q .; then
  printf 'FAIL command=./scripts/kind-smoke.sh reason=mock-gateway endpoints still present after scale-to-zero\n' >&2
  exit 1
fi
while kubectl -n "$NAMESPACE" get pods -l app.kubernetes.io/component=mock-gateway \
  --field-selector=status.phase=Running -o name 2>/dev/null | grep -q .; do
  sleep 2
done
kubectl -n "$NAMESPACE" delete job "${RELEASE}-conformance-gateway-down" --ignore-not-found --wait=true

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

for _ in {1..30}; do
  if kubectl -n "$NAMESPACE" get "job/${RELEASE}-conformance-gateway-down" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

set +e
kubectl -n "$NAMESPACE" wait --for=condition=failed "job/${RELEASE}-conformance-gateway-down" --timeout=180s
gateway_down_wait=$?
set -e
if ((gateway_down_wait != 0)); then
  if kubectl -n "$NAMESPACE" get "job/${RELEASE}-conformance-gateway-down" -o jsonpath='{.status.succeeded}' 2>/dev/null | grep -q '^1'; then
    printf 'FAIL command=./scripts/kind-smoke.sh reason=gateway-down job succeeded unexpectedly\n' >&2
    kubectl -n "$NAMESPACE" logs "job/${RELEASE}-conformance-gateway-down" >&2 || true
    exit 1
  fi
  if ! kubectl -n "$NAMESPACE" get "job/${RELEASE}-conformance-gateway-down" -o jsonpath='{.status.failed}' 2>/dev/null | grep -q '^[1-9]'; then
    printf 'FAIL command=./scripts/kind-smoke.sh reason=gateway-down job did not fail\n' >&2
    kubectl -n "$NAMESPACE" get "job/${RELEASE}-conformance-gateway-down" -o yaml >&2 || true
    kubectl -n "$NAMESPACE" logs "job/${RELEASE}-conformance-gateway-down" >&2 || true
    exit 1
  fi
fi
kubectl -n "$NAMESPACE" logs "job/${RELEASE}-conformance-gateway-down" | tee "$GATEWAY_DOWN_LOG"
if ! grep -F '"code":"TQX_HOST_RETRY_EXHAUSTED"' "$GATEWAY_DOWN_LOG" >/dev/null; then
  printf 'FAIL command=./scripts/kind-smoke.sh reason=gateway-down missing retry-exhausted diagnostic\n' >&2
  exit 1
fi
printf 'kind smoke: gateway_down=true code=TQX_HOST_RETRY_EXHAUSTED log=%s\n' "$GATEWAY_DOWN_LOG"
