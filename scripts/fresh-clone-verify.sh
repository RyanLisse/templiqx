#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="$REPO_ROOT/artifacts/fresh-clone"
WORKTREE="${FRESH_CLONE_WORKTREE:-}"
CARGO_HOME="${FRESH_CLONE_CARGO_HOME:-}"
CLEANUP_WORKTREE=0

mkdir -p "$ARTIFACT_DIR"
LOG="$ARTIFACT_DIR/fresh-clone.log"
exec > >(tee -a "$LOG") 2>&1

cleanup() {
  if ((CLEANUP_WORKTREE == 1)) && [[ -n $WORKTREE ]] && [[ -d $WORKTREE ]]; then
    git -C "$REPO_ROOT" worktree remove --force "$WORKTREE" >/dev/null 2>&1 || rm -rf "$WORKTREE"
  fi
}
trap cleanup EXIT

if [[ -z $WORKTREE ]]; then
  WORKTREE="$(mktemp -d "${TMPDIR:-/tmp}/templiqx-fresh-clone.XXXXXX")"
  git -C "$REPO_ROOT" worktree add --detach "$WORKTREE" HEAD >/dev/null
  CLEANUP_WORKTREE=1
fi

if [[ -z $CARGO_HOME ]]; then
  CARGO_HOME="$WORKTREE/.cargo-home"
fi

export CARGO_HOME
export CARGO_TARGET_DIR="$WORKTREE/target"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
rm -rf "$CARGO_HOME" "$CARGO_TARGET_DIR"
mkdir -p "$CARGO_HOME"

printf 'fresh-clone: worktree=%s cargo_home=%s target=%s\n' "$WORKTREE" "$CARGO_HOME" "$CARGO_TARGET_DIR"

cd "$WORKTREE"
cargo fetch
# Skip qlty here: linting is gated by the dedicated `qlty` CI job, and a cold
# clone re-initializes all qlty plugins from scratch which blows this job's time
# budget. Fresh-clone proves the checkout builds and tests reproducibly.
export SKIP_QLTY=1
just verify

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  just verify-deploy
else
  if [[ ${CI:-} == "true" ]]; then
    printf 'FAIL fresh-clone: Docker required in CI for verify-deploy\n' >&2
    exit 1
  fi
  printf 'SKIP_ENV fresh-clone: Docker unavailable; verify-deploy skipped\n'
fi

printf 'fresh-clone: ok log=%s\n' "$LOG"
