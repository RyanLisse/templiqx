#!/usr/bin/env bash
set -euo pipefail

chart="${1:?usage: verify-packaged-chart.sh CHART.tgz IMAGE_REPOSITORY VERSION DIGEST}"
repository="${2:?missing image repository}"
version="${3:?missing image version}"
digest="${4:?missing image digest}"

fail() {
  printf 'packaged chart verification: FAIL %s\n' "$*" >&2
  exit 1
}

command -v helm >/dev/null 2>&1 || fail "helm is required"
[[ -f $chart ]] || fail "chart archive not found: $chart"
[[ $digest =~ ^sha256:[a-f0-9]{64}$ ]] || fail "invalid image digest: $digest"

values="$(helm show values "$chart")"
grep -Fq "repository: $repository" <<<"$values" || fail "default repository is not published conformance image"
grep -Fq "tag: $version" <<<"$values" || fail "default tag does not match release version"
grep -Fq "digest: $digest" <<<"$values" || fail "default digest does not match verified conformance image"
grep -A1 '^mock:' <<<"$values" | grep -Fq 'enabled: true' || fail "mock gateway is not enabled by default"

rendered="$(helm template templiqx-release "$chart")"
image="$repository@$digest"
[[ $(grep -Fc "image: \"$image\"" <<<"$rendered") -eq 9 ]] ||
  fail "expected the immutable published image in one gateway and eight conformance jobs"
[[ $(grep -c '^kind: Job$' <<<"$rendered") -eq 8 ]] || fail "expected eight conformance Jobs"
grep -Fq 'kind: Deployment' <<<"$rendered" || fail "mock gateway Deployment missing"
grep -Fq 'kind: Service' <<<"$rendered" || fail "mock gateway Service missing"

# Exercise Helm's install path as well as plain template rendering. This stays
# client-side and does not require a Kubernetes cluster.
helm install templiqx-release "$chart" --dry-run=client --debug >/dev/null
printf 'packaged chart verification: OK image=%s jobs=8 gateway=enabled\n' "$image"
