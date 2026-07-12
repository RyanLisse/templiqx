<!-- OPENWIKI:START -->

## OpenWiki

Recurring code documentation lives under `openwiki/` (start at `openwiki/quickstart.md`). The scheduled GitHub Actions workflow regenerates it — do not hand-edit OpenWiki pages unless explicitly asked; update source or `docs/` instead.

<!-- OPENWIKI:END -->

## Learned User Preferences

- For product or architecture scope, anchor to the Linear project **Basenet CRM3** (`BLI-*` keys) rather than stale repo docs alone.
- Keep normative specs, ADRs, and plans under `docs/` with navigation via [`docs/README.md`](docs/README.md). Root holds entry points (`AGENTS.md`, `CLAUDE.md`) only.
- Host-owned concerns (auth, tenant policy, approval, retrieval, secrets, provider SDKs) belong in the Basenet host — implement typed ports here, not host wiring in core crates.
- Run `just verify` before PRs; run `qlty fmt` and `qlty check --fix --level=low` before commit (CI enforces both).
- Preserve CRM3 evidence-grounding in conformance scenarios — draft output must stay grounded in source fragments.

## Learned Workspace Facts

- **Purpose:** provider-neutral AI interaction contract compiler POC (`templiqx/v1alpha1`) for pre-CRM3 readiness.
- **Canonical service:** `TempliqxService` in `templiqx-application`; CLI and MCP are thin transports over the same capability catalog — no separate agent path.
- **Composition today:** `templiqx-local` is the only concrete wiring (filesystem storage + deterministic fake adapters).
- **Boundaries:** `./scripts/check-boundaries.sh` is enforced in CI; a passing `cargo build` does not catch dependency violations.
- **Mocks are conformance-only:** `templiqx-mock`, `templiqx-runtime-http-mock`, and `templiqx-mock-gateway` must not appear in the default CLI/MCP/application graph.
- **CRM3 proof:** synthetic fixture at `examples/crm3`; scenarios under `examples/crm3/scenarios/**`; tests in `templiqx-conformance`.
- **Deployment:** Docker (`Dockerfile`, `deploy/compose.yml`), Helm (`charts/templiqx/`), smoke scripts (`scripts/docker-smoke.sh`, `scripts/kind-smoke.sh`, `scripts/supply-chain-smoke.sh`).
