#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'FAIL ci-gate: %s\n' "$*" >&2
  exit 1
}

# A missing `rg` binary makes `if rg ...; then` a no-op (same failure mode as
# "no match"), which would silently disable the ignored-test gate below.
command -v rg >/dev/null 2>&1 || fail "ripgrep (rg) is required to run CI gates"

if rg -n '#\[ignore\]' --glob '*.rs' crates adapters tools >/tmp/templiqx-ci-ignored-tests.txt 2>/dev/null; then
  cat /tmp/templiqx-ci-ignored-tests.txt >&2
  fail 'ignored Rust tests are not allowed in CI'
fi

if [[ -n ${ALLOW_GOLDEN_UPDATE:-} ]]; then
  printf 'ci gates: golden review bypass via ALLOW_GOLDEN_UPDATE\n'
  exit 0
fi

base_ref="${GITHUB_BASE_REF:-}"
if [[ -n $base_ref ]]; then
  git fetch origin "$base_ref" --depth=1 >/dev/null 2>&1 || true
  if git rev-parse --verify "origin/$base_ref" >/dev/null 2>&1; then
    diff_range="origin/$base_ref...HEAD"
  else
    diff_range="HEAD~1..HEAD"
  fi
else
  diff_range="HEAD~1..HEAD"
fi

golden_changes="$(git diff --name-only "$diff_range" -- \
  'scripts/golden/' \
  'examples/crm3/scenarios/' 2>/dev/null || true)"
if [[ -n $golden_changes ]]; then
  if ! git log "$diff_range" --format=%B 2>/dev/null | rg -q 'GOLDEN_REVIEW:'; then
    printf 'changed golden paths without GOLDEN_REVIEW commit marker:\n%s\n' "$golden_changes" >&2
    fail 'golden fixture updates require GOLDEN_REVIEW: in commit message or ALLOW_GOLDEN_UPDATE=1'
  fi
fi

printf 'ci gates: ok\n'
