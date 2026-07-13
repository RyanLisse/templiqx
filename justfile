verify:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    ./scripts/check-boundaries.sh
    ./scripts/check-ci-gates.sh
    # qlty is skippable (SKIP_QLTY) so the fresh-clone reproducibility gate can
    # omit it — linting is already covered by the dedicated `qlty` CI job, and a
    # cold-clone qlty re-init blows the fresh-clone time budget. Local runs still lint.
    [ -n "${SKIP_QLTY:-}" ] || qlty check --level=low

verify-deploy:
    helm lint charts/templiqx -f charts/templiqx/values-mock.yaml
    ./scripts/docker-smoke.sh
    ./scripts/kind-smoke.sh
    ./scripts/supply-chain-smoke.sh
    ./scripts/check-boundaries.sh

fresh-clone:
    ./scripts/fresh-clone-verify.sh
