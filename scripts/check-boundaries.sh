#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'FAIL boundary: %s\n' "$*" >&2
  exit 1
}

# The checks below rely on `rg` and silently treat "no match" and "command
# not found" the same way (both make an `if rg ...; then` block a no-op) —
# so a missing binary would make every boundary check pass without actually
# checking anything. Fail closed instead.
command -v rg >/dev/null 2>&1 || fail "ripgrep (rg) is required to run boundary checks"

require_path() {
  [[ -e $1 ]] || fail "missing expected target: $1"
}

for crate in templiqx-contracts templiqx-ports templiqx-core; do
  manifest="crates/$crate/Cargo.toml"
  if grep -Eiq '(openai|anthropic|gemini|bedrock|basenet|crm3|rmcp)' "$manifest"; then
    fail "forbidden dependency in $manifest"
  fi
done

for crate in templiqx-application templiqx-cli templiqx-mcp templiqx-http-server; do
  manifest="crates/$crate/Cargo.toml"
  if grep -Eq 'templiqx-mock|templiqx-runtime-http-mock|templiqx-mock-gateway' "$manifest"; then
    fail "default composition depends on conformance mock: $manifest"
  fi
done

# Production HTTP transport is allowed to own HTTP server dependencies, but
# it must remain a thin TempliqxService surface and never import conformance mocks.
if [[ -e crates/templiqx-http ]]; then
  require_path crates/templiqx-http/Cargo.toml
  if rg -n -i \
    '(templiqx[-_]?mock|templiqx[-_]?runtime[-_]?http[-_]?mock|templiqx[-_]?mock[-_]?gateway|mock[-_]?gateway)' \
    crates/templiqx-http/Cargo.toml crates/templiqx-http/src \
    >/tmp/templiqx-boundary-http-prod.txt 2>/dev/null; then
    cat /tmp/templiqx-boundary-http-prod.txt >&2
    fail "production templiqx-http must not depend on conformance mocks"
  fi
fi

# The host-owned production server may wire real runtime adapters, but it must
# remain completely isolated from synthetic conformance mocks.
if [[ -e crates/templiqx-http-server ]]; then
  require_path crates/templiqx-http-server/Cargo.toml
  if rg -n -i \
    '(templiqx[-_]?mock|templiqx[-_]?runtime[-_]?http[-_]?mock|templiqx[-_]?mock[-_]?gateway|mock[-_]?gateway)' \
    crates/templiqx-http-server/Cargo.toml crates/templiqx-http-server/src \
    >/tmp/templiqx-boundary-http-server-prod.txt 2>/dev/null; then
    cat /tmp/templiqx-boundary-http-server-prod.txt >&2
    fail "templiqx-http-server (local/demo Operations binary) must not depend on conformance mocks"
  fi
fi

require_path openapi/templiqx-operations-v1.yaml

# HTTP transport mocks are edge-only concerns. Keep the core, contracts,
# ports, application, CLI, and MCP surfaces free of HTTP client/server mock
# crates and implementations.
for crate in templiqx-core templiqx-contracts templiqx-ports templiqx-application templiqx-cli templiqx-mcp; do
  root="crates/$crate"
  if rg -n -i \
    '(reqwest|ureq|hyper|axum|warp|actix[-_]web|httpmock|wiremock|mockito|mock[-_]?server|mock[-_]?gateway|runtime[-_]?http[-_]?mock|templiqx[-_]runtime[-_]http[-_]mock)' \
    "$root/Cargo.toml" "$root/src" \
    >/tmp/templiqx-boundary-http-mock.txt 2>/dev/null; then
    cat /tmp/templiqx-boundary-http-mock.txt >&2
    fail "HTTP mock transport leaked into default surface: $root"
  fi
done

# A mock gateway may be referenced only by the conformance tool and adapter
# trees.  This catches future path/import composition even when the package
# name is changed.
if rg -n -i \
  '(templiqx[-_]?mock[-_]?gateway|templiqx[-_]?runtime[-_]?http[-_]?mock|runtime[-_]?http[-_]?mock)' \
  crates/templiqx-core crates/templiqx-contracts crates/templiqx-ports \
  crates/templiqx-application crates/templiqx-cli crates/templiqx-mcp \
  crates/templiqx-http-server \
  >/tmp/templiqx-boundary-mock-composition.txt; then
  cat /tmp/templiqx-boundary-mock-composition.txt >&2
  fail "HTTP mock gateway composition leaked outside tools/adapters"
fi

if rg -n '(approval|permission|tenant|retrieval|queue|audit)' \
  crates/templiqx-contracts crates/templiqx-core crates/templiqx-ports >/tmp/templiqx-boundary-core.txt; then
  cat /tmp/templiqx-boundary-core.txt >&2
  fail "host-owned vocabulary leaked into core contracts/ports"
fi

require_path crates/templiqx-mock/Cargo.toml
require_path tools/templiqx-mock-gateway/Cargo.toml
require_path tools/templiqx-mock-gateway/src/main.rs
require_path crates/templiqx-conformance/tests/crm3_failures.rs
require_path deploy/compose.yml
require_path charts/templiqx/values-mock.yaml
require_path scripts/docker-smoke.sh
require_path scripts/kind-smoke.sh
require_path scripts/check-ci-gates.sh
require_path scripts/golden/http-conformance.json

if [[ -f Dockerfile ]] && awk '
  /^FROM / {
    image = $2
    if (image ~ /^--platform=/) {
      image = $3
    }
    if (image != "scratch" && image !~ /@sha256:/ && !stage[image]) {
      print FNR ":" $0
    }
    if ($(NF - 1) == "AS") {
      stage[$NF] = 1
    }
  }
' Dockerfile >/tmp/templiqx-boundary-docker.txt && [[ -s /tmp/templiqx-boundary-docker.txt ]]; then
  cat /tmp/templiqx-boundary-docker.txt >&2
  fail "Dockerfile base image must be scratch or pinned by digest"
fi

# Release images are separate trust and deployment artifacts. Keep product
# stages minimal at the Dockerfile boundary instead of relying only on a smoke
# test that may not run on every developer machine.
docker_stage() {
  awk -v target="$1" '
    /^FROM / { active = ($NF == target) }
    active { print }
  ' Dockerfile
}

for target in templiqx-cli templiqx-mcp templiqx-http-server templiqx-conformance; do
  if ! rg -q "^FROM .* AS ${target}$" Dockerfile; then
    fail "Dockerfile is missing explicit image target: $target"
  fi
done
rg -Fq 'FROM source AS http-server-builder' Dockerfile ||
  fail "Dockerfile is missing the product-only HTTP server builder"
rg -Fq 'cargo build --release -p templiqx-http-server' Dockerfile ||
  fail "HTTP server builder does not build templiqx-http-server"

for target in templiqx-cli templiqx-mcp templiqx-http-server; do
  stage_file="/tmp/templiqx-boundary-${target}.txt"
  docker_stage "$target" >"$stage_file"

  if rg -n '(templiqx-mock-gateway|templiqx-http-conformance|/packages|from=conformance-builder)' \
    "$stage_file" >/tmp/templiqx-boundary-product-image.txt; then
    cat /tmp/templiqx-boundary-product-image.txt >&2
    fail "conformance binary or fixture leaked into product image target: $target"
  fi

  copy_count="$(rg -c '^COPY .* /usr/local/bin/' "$stage_file" || true)"
  [[ $copy_count == 1 ]] || fail "$target must copy exactly one product binary (found $copy_count)"

  # These are intentionally literal Dockerfile build-argument references.
  # shellcheck disable=SC2016
  for label in \
    'org.opencontainers.image.source="https://github.com/RyanLisse/templiqx"' \
    'org.opencontainers.image.version=$VERSION' \
    'org.opencontainers.image.revision=$VCS_REF'; do
    rg -Fq "$label" "$stage_file" || fail "$target is missing OCI label: $label"
  done
done

cli_stage="$(docker_stage templiqx-cli)"
mcp_stage="$(docker_stage templiqx-mcp)"
http_server_stage="$(docker_stage templiqx-http-server)"
[[ $cli_stage == *'/target/release/templiqx /usr/local/bin/templiqx'* ]] ||
  fail "CLI image does not copy the CLI binary"
[[ $mcp_stage == *'/target/release/templiqx-mcp /usr/local/bin/templiqx-mcp'* ]] ||
  fail "MCP image does not copy the MCP binary"
[[ $http_server_stage == *'/target/release/templiqx-http-server /usr/local/bin/templiqx-http-server'* ]] ||
  fail "HTTP server image does not copy the HTTP server binary"
[[ $http_server_stage == *'EXPOSE 8080'* ]] ||
  fail "HTTP server image does not expose port 8080"

conformance_stage="$(docker_stage templiqx-conformance)"
for required in templiqx-mock-gateway templiqx-http-conformance '/packages' \
  'io.templiqx.artifact.class="synthetic-conformance-only"'; do
  [[ $conformance_stage == *"$required"* ]] ||
    fail "conformance image is missing required content: $required"
done
if [[ $conformance_stage == *'/target/release/templiqx /usr/local/bin/templiqx'* ]] ||
  [[ $conformance_stage == *'/target/release/templiqx-mcp /usr/local/bin/templiqx-mcp'* ]] ||
  [[ $conformance_stage == *'/target/release/templiqx-http-server /usr/local/bin/templiqx-http-server'* ]]; then
  fail "product binary leaked into conformance image"
fi

if rg -n '^\s*image:' deploy/compose.yml |
  rg -v '(\$\{CONFORMANCE_IMAGE:-templiqx-conformance:pre-crm3\}|\$\{HTTP_SERVER_IMAGE:-templiqx-http-server:pre-crm3\})' \
    >/tmp/templiqx-boundary-compose-image.txt; then
  cat /tmp/templiqx-boundary-compose-image.txt >&2
  fail "Compose mock profiles must use the explicit conformance image"
fi
rg -Fq 'image: ${HTTP_SERVER_IMAGE:-templiqx-http-server:pre-crm3}' deploy/compose.yml ||
  fail "Compose HTTP server must use the explicit product image"

rg -Fq 'io.templiqx.chart.class: product-and-conformance' charts/templiqx/Chart.yaml ||
  fail "Helm chart must identify its separate product and conformance surfaces"

if [[ -f charts/templiqx/values-mock.yaml ]]; then
  if rg -n 'mcp|LoadBalancer|NodePort' charts/templiqx/templates >/tmp/templiqx-boundary-helm.txt; then
    cat /tmp/templiqx-boundary-helm.txt >&2
    fail "chart must not expose MCP or a non-ClusterIP service"
  fi
  if rg -n 'kind: Service' charts/templiqx/templates |
    rg -v '(mock-gateway.yaml|http-server.yaml)' >/tmp/templiqx-boundary-helm.txt; then
    cat /tmp/templiqx-boundary-helm.txt >&2
    fail "only the product HTTP server and mock gateway may define a Kubernetes Service"
  fi
  rg -Fq 'httpServer:' charts/templiqx/values-mock.yaml ||
    fail "mock values must explicitly configure the product HTTP server toggle"
  if rg -n -i '(templiqx[-_]?mock|mock[-_]?gateway|conformance)' \
    charts/templiqx/templates/http-server.yaml >/tmp/templiqx-boundary-helm-product.txt; then
    cat /tmp/templiqx-boundary-helm-product.txt >&2
    fail "conformance content leaked into the Helm product HTTP server"
  fi
fi

printf 'dependency and deployment boundaries: ok\n'
