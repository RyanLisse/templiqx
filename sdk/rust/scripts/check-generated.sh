#!/usr/bin/env bash
set -euo pipefail

sdk_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(cd "$sdk_root/../.." && pwd)"

"$sdk_root/scripts/generate.sh" --check
git -C "$repo_root" diff --exit-code -- sdk/rust/src/generated.rs
