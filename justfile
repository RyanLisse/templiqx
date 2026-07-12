verify:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    ./scripts/check-boundaries.sh
    ./scripts/check-ci-gates.sh
    qlty check --level=low

verify-deploy:
    helm lint charts/templiqx -f charts/templiqx/values-mock.yaml
    ./scripts/docker-smoke.sh
    ./scripts/kind-smoke.sh
    ./scripts/supply-chain-smoke.sh
    ./scripts/check-boundaries.sh

fresh-clone:
    ./scripts/fresh-clone-verify.sh
