#!/usr/bin/env bash
set -euo pipefail

image="${1:?usage: release-verify-oci.sh IMAGE TAG DIGEST CERTIFICATE_IDENTITY}"
tag="${2:?missing tag}"
digest="${3:?missing digest}"
certificate_identity="${4:?missing certificate identity}"
issuer="https://token.actions.githubusercontent.com"
tagged="$image:$tag"
immutable="$image@$digest"

[[ $digest == sha256:* ]] || {
  printf 'release OCI verification: invalid digest %s\n' "$digest" >&2
  exit 1
}

resolved="$(docker buildx imagetools inspect "$tagged" | awk '$1 == "Digest:" { print $2; exit }')"
[[ $resolved == "$digest" ]] || {
  printf 'release OCI verification: tag/digest mismatch tag=%s expected=%s actual=%s\n' \
    "$tagged" "$digest" "$resolved" >&2
  exit 1
}

platforms="$(docker buildx imagetools inspect --raw "$immutable" |
  jq -r '.manifests[]?.platform | select(.os == "linux") | "\(.os)/\(.architecture)"' |
  sort -u)"
[[ $platforms == $'linux/amd64\nlinux/arm64' ]] || {
  printf 'release OCI verification: unexpected platforms for %s:\n%s\n' "$immutable" "$platforms" >&2
  exit 1
}

cosign sign --yes "$immutable"
cosign verify \
  --certificate-identity "$certificate_identity" \
  --certificate-oidc-issuer "$issuer" \
  "$immutable" >/dev/null

printf 'release OCI verification: OK image=%s platforms=linux/amd64,linux/arm64\n' "$immutable"
