verify:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    ./scripts/check-boundaries.sh
    npm run openapi:validate
    just compat-check
    just bump-check
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

conformance-http:
    #!/usr/bin/env bash
    set -euo pipefail

    # The golden is intentionally the intake object (option 1), not an 8-way aggregate.
    root="$(pwd)"
    golden="${TEMPLIQX_HTTP_CONFORMANCE_GOLDEN:-$root/scripts/golden/http-conformance.json}"
    tmp_dir="$(mktemp -d)"
    gateway_pid=""
    cleanup() {
        if [ -n "$gateway_pid" ]; then
            kill "$gateway_pid" 2>/dev/null || true
            wait "$gateway_pid" 2>/dev/null || true
        fi
        rm -rf "$tmp_dir"
    }
    trap cleanup EXIT

    cargo build --quiet -p templiqx-mock-gateway -p templiqx-http-conformance
    gateway="$root/target/debug/templiqx-mock-gateway"
    runner="$root/target/debug/templiqx-http-conformance"
    "$gateway" \
        --listen 127.0.0.1:18080 \
        --scenario-root "$root/examples/crm3/scenarios" \
        >"$tmp_dir/gateway.log" 2>&1 &
    gateway_pid=$!
    for _ in $(seq 1 30); do
        if curl -fsS http://127.0.0.1:18080/health/ready >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done
    curl -fsS http://127.0.0.1:18080/health/ready >/dev/null

    scenarios=()
    while IFS= read -r scenario; do
        scenarios+=("$scenario")
    done < <(jq -r '.scenarios[].id' "$root/examples/crm3/scenarios/inventory.json")
    [ "${#scenarios[@]}" -eq 8 ]
    for scenario in "${scenarios[@]}"; do
        receipt="$tmp_dir/http-${scenario}.json"
        TEMPLIQX_RUNTIME_URL=http://127.0.0.1:18080 \
        TEMPLIQX_RUNTIME_SCENARIO="$scenario" \
            "$runner" | tee "$receipt"
        jq -e --arg scenario "$scenario" '.ok == true and .scenario_id == $scenario' "$receipt" >/dev/null
    done

    jq -S . "$tmp_dir/http-intake-document-01.json" >"$tmp_dir/intake.sorted.json"
    jq -S . "$golden" >"$tmp_dir/golden.sorted.json"
    cmp "$tmp_dir/intake.sorted.json" "$tmp_dir/golden.sorted.json"
    echo "intake receipt matches golden"
    cargo test -p templiqx-conformance --test crm3

# Complete local release gate. Hosted CI deliberately runs only `verify` plus
# the lightweight docs build; expensive deployment checks stay local-first.
verify-all: verify docs-build verify-deploy

fresh-clone:
    ./scripts/fresh-clone-verify.sh

docs-dev:
    npm run dev

docs-build:
    npm run build

openapi-validate:
    npm run openapi:validate

compat-check:
    npm run openapi:compat

bump-check:
    npm run openapi:bump-check

bump-engine *args:
    node scripts/bump-engine-version.mjs {{args}}

openapi-typescript-proof:
    npm run openapi:typescript-proof
