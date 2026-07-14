verify:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    ./scripts/check-boundaries.sh
    ./scripts/check-ci-gates.sh
    # qlty is skippable (SKIP_QLTY) only for constrained cold-clone checks.
    # Normal local verification and the minimal hosted CI backstop both lint.
    [ -n "${SKIP_QLTY:-}" ] || qlty check --level=low

verify-deploy:
    helm lint charts/templiqx -f charts/templiqx/values-mock.yaml
    ./scripts/docker-smoke.sh
    ./scripts/kind-smoke.sh
    ./scripts/supply-chain-smoke.sh
    ./scripts/check-boundaries.sh

# Complete local release gate. Hosted CI deliberately runs only `verify`.
verify-all: verify verify-deploy

fresh-clone:
    ./scripts/fresh-clone-verify.sh
