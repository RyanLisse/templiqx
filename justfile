verify:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    ./scripts/check-boundaries.sh

verify-deploy:
    ./scripts/docker-smoke.sh
    ./scripts/kind-smoke.sh
    ./scripts/supply-chain-smoke.sh
    ./scripts/check-boundaries.sh
