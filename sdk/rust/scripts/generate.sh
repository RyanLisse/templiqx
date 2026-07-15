#!/usr/bin/env bash
set -euo pipefail

sdk_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo run --quiet --manifest-path "$sdk_root/Cargo.toml" --example generate -- "$@"
