#!/usr/bin/env sh
set -eu

sdk_root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
repo_root="$(CDPATH='' cd -- "$sdk_root/../.." && pwd)"

cd "$sdk_root"
go generate ./...
go run ./scripts/generate.go --check

cd "$repo_root"
git diff --exit-code -- sdk/go/operations_v1.gen.go sdk/go/compat_generated.go
