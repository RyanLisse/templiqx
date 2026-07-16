#!/usr/bin/env bash
# Verify generated Operations API SDKs (drift + build/typecheck/unit tests).
# TypeScript is always required. Other languages run when their toolchain is
# present unless VERIFY_SDK_STRICT=1 (then missing tools fail).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

strict="${VERIFY_SDK_STRICT:-0}"
languages="${VERIFY_SDK_LANGS:-typescript go rust python dotnet}"
failed=0
ran=0

require_or_skip() {
  local tool="$1"
  if command -v "$tool" >/dev/null 2>&1; then
    return 0
  fi
  if [ "$strict" = "1" ]; then
    echo "FAIL verify-sdks: required tool missing: $tool" >&2
    return 1
  fi
  echo "SKIP: $tool not on PATH"
  return 2
}

run_typescript() {
  echo "==> TypeScript SDK (sdk/typescript)"
  (
    cd sdk/typescript
    if [ ! -d node_modules ]; then
      if [ -f package-lock.json ]; then
        npm ci --ignore-scripts
      else
        npm install --ignore-scripts
      fi
    fi
    npm run generate:check
    npm run typecheck
    npm run build
    npm test
  )
}

run_go() {
  echo "==> Go SDK (sdk/go)"
  require_or_skip go || return $?
  (
    cd sdk/go
    ./scripts/check-generated.sh
    go test ./...
  )
}

run_rust() {
  echo "==> Rust SDK (sdk/rust)"
  require_or_skip cargo || return $?
  (
    cd sdk/rust
    ./scripts/check-generated.sh
    cargo clippy --all-targets -- -D warnings
    cargo test
  )
}

run_python() {
  echo "==> Python SDK (sdk/python)"
  if ! command -v uv >/dev/null 2>&1; then
    if [ "$strict" = "1" ]; then
      echo "FAIL verify-sdks: required tool missing: uv" >&2
      return 1
    fi
    echo "SKIP: uv not on PATH"
    return 2
  fi
  (
    cd sdk/python
    uv run python scripts/generate.py --check
    uv run pytest
  )
}

run_dotnet() {
  echo "==> .NET SDK (sdk/dotnet)"
  require_or_skip dotnet || return $?
  (
    cd sdk/dotnet
    ./scripts/generate.sh --check
    dotnet test Templiqx.Adapter.slnx --nologo
  )
}

for lang in $languages; do
  status=0
  case "$lang" in
  typescript) run_typescript || status=$? ;;
  go) run_go || status=$? ;;
  rust) run_rust || status=$? ;;
  python) run_python || status=$? ;;
  dotnet) run_dotnet || status=$? ;;
  *)
    echo "FAIL verify-sdks: unknown language '$lang'" >&2
    status=1
    ;;
  esac
  if [ "$status" -eq 0 ]; then
    ran=$((ran + 1))
  elif [ "$status" -eq 2 ]; then
    :
  else
    failed=1
  fi
done

if [ "$failed" -ne 0 ]; then
  echo "FAIL verify-sdks: one or more SDK gates failed" >&2
  exit 1
fi

echo "verify-sdks ok (${ran} language suite(s) ran)"
