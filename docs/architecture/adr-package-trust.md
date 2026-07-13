# ADR: Package trust v1

## Status

Accepted (2026-07-12)

## Context

Pre-CRM3 readiness requires verifiable package identity beyond deterministic
hashes (R18). Host publication and tamper-evidence need a signing model that
remains backward compatible with unsigned dev packages.

## Decision

1. **Manifest-level detached signatures** — Sign the canonical package identity
   JSON (`manifest` without `signatures` + sorted artifact hashes, including the
   optional `templiqx.lock`), not individual files.
2. **Development algorithm** — `sha256-keyed` with
   `TEMPLIQX_PACKAGE_SIGNING_KEY` exists only for local development and CI
   conformance. It is a keyed digest, not a public-key signature, must not be
   described as production signing, and must not be used as an OCI trust root.
   Production OCI publication uses Sigstore/Cosign keyless OIDC and verifies the
   image digest separately.
3. **Schema** — Optional `signatures` array on `templiqx.yaml`:

   ```yaml
   signatures:
     - key_id: ci-test
       algorithm: sha256-keyed
       value: <hex digest>
   ```

4. **Validation behavior**
   - Unsigned packages: pass default validation.
   - Explicit strict verification (or `TEMPLIQX_PACKAGE_STRICT=1` during package
     validation): reject unsigned packages with `TQX_PACKAGE_UNSIGNED` error.
   - Signatures present + key set: verify or emit `TQX_PACKAGE_SIGNATURE_INVALID`.
   - Signatures present + key unset: `TQX_PACKAGE_SIGNATURE_UNVERIFIED` error.
   - Duplicate signature identities and unsupported algorithms fail closed.
   - The digest binds the canonical identity, `key_id`, and `algorithm`; changing
     signature metadata invalidates it. If any supported signature is invalid,
     the complete signature set is rejected (no partial acceptance).

## Operator flow

1. Run `export-package-identity <package>` and retain its
   `package_identity` fingerprint as review evidence. Use the returned
   `manifest` fingerprint as the signing CAS value.
2. Set `TEMPLIQX_PACKAGE_SIGNING_KEY` from a local/CI secret store. Never pass
   the key as a CLI or MCP argument.
3. Run `sign-package <package> --key-id <id> --expected-fingerprint <manifest-fingerprint>`.
   The store rechecks both the manifest CAS value and full package-identity
   fingerprint while holding its package lock. This prevents attachment after
   an unseen manifest or artifact mutation; an existing signature for the same
   key and algorithm is replaced.
4. Run `verify-package-trust <package> --strict`. Artifact tampering, wrong
   keys, cross-package replay, unsigned packages, duplicates and unsupported
   algorithms all produce error diagnostics.
5. For a release, independently sign and verify the OCI digest with Cosign.
   Manifest trust and OCI distribution trust are deliberately separate gates.

## Consequences

- Dev workflows unchanged for unsigned packages.
- CI can round-trip sign/verify synthetic packages without cosign registry push.
- Cosign image attestation remains separate (BuildKit provenance in
  `scripts/supply-chain-smoke.sh`).

## Alternatives considered

- **Per-file signatures** — Rejected; inventory hash already covers artifact set.
- **Mandatory signing in dev** — Rejected; blocks local iteration.
