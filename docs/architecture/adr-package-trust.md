# ADR: Package trust v1

## Status

Accepted (2026-07-12)

## Context

Pre-CRM3 readiness requires verifiable package identity beyond deterministic
hashes (R18). Host publication and tamper-evidence need a signing model that
remains backward compatible with unsigned dev packages.

## Decision

1. **Manifest-level detached signatures** — Sign the canonical package identity
   JSON (`manifest` without `signatures` + sorted artifact hashes), not individual
   files.
2. **Stub algorithm** — `sha256-keyed` with `TEMPLIQX_PACKAGE_SIGNING_KEY` for
   local/CI verification. Production path aligns with Sigstore/cosign keyless
   OIDC in GitHub Actions for OCI artifacts (see supply-chain job).
3. **Schema** — Optional `signatures` array on `templiqx.yaml`:

   ```yaml
   signatures:
     - key_id: ci-test
       algorithm: sha256-keyed
       value: <hex digest>
   ```

4. **Validation behavior**
   - Unsigned packages: pass default validation.
   - `TEMPLIQX_PACKAGE_STRICT=1`: emit `TQX_PACKAGE_UNSIGNED` warning.
   - Signatures present + key set: verify or emit `TQX_PACKAGE_SIGNATURE_INVALID`.
   - Signatures present + key unset: `TQX_PACKAGE_SIGNATURE_UNVERIFIED` error.

## Consequences

- Dev workflows unchanged for unsigned packages.
- CI can round-trip sign/verify synthetic packages without cosign registry push.
- Cosign image attestation remains separate (BuildKit provenance in
  `scripts/supply-chain-smoke.sh`).

## Alternatives considered

- **Per-file signatures** — Rejected; inventory hash already covers artifact set.
- **Mandatory signing in dev** — Rejected; blocks local iteration.
