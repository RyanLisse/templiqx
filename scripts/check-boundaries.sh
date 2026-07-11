#!/usr/bin/env bash
set -euo pipefail
for crate in templiqx-contracts templiqx-ports templiqx-core; do
  manifest="crates/$crate/Cargo.toml"
  if grep -Eiq '(openai|anthropic|gemini|bedrock|basenet|crm3|mcp)' "$manifest"; then
    echo "forbidden dependency in $manifest" >&2
    exit 1
  fi
done
echo "dependency boundaries: ok"
