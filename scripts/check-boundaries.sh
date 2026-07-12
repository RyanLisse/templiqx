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

for crate in templiqx-application templiqx-cli templiqx-mcp; do
  manifest="crates/$crate/Cargo.toml"
  if grep -Eq 'templiqx-mock|templiqx-runtime-http-mock|templiqx-mock-gateway' "$manifest"; then
    fail "default composition depends on conformance mock: $manifest"
  fi
done

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
    if (image != "scratch" && image !~ /@sha256:/) {
      print FNR ":" $0
    }
  }
' Dockerfile >/tmp/templiqx-boundary-docker.txt && [[ -s /tmp/templiqx-boundary-docker.txt ]]; then
  cat /tmp/templiqx-boundary-docker.txt >&2
  fail "Dockerfile base image must be scratch or pinned by digest"
fi

if [[ -f charts/templiqx/values-mock.yaml ]]; then
  if rg -n 'mcp|LoadBalancer|NodePort' charts/templiqx/templates >/tmp/templiqx-boundary-helm.txt; then
    cat /tmp/templiqx-boundary-helm.txt >&2
    fail "chart must not expose MCP or a general HTTP service"
  fi
  if rg -n 'kind: Service' charts/templiqx/templates | rg -v 'mock-gateway.yaml' >/tmp/templiqx-boundary-helm.txt; then
    cat /tmp/templiqx-boundary-helm.txt >&2
    fail "only the mock gateway may define a Kubernetes Service"
  fi
fi

printf 'dependency and deployment boundaries: ok\n'
