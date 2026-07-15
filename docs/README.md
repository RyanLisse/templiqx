# Templiqx documentation

Published docs site: [ryanlisse.github.io/templiqx](https://ryanlisse.github.io/templiqx) (Blume). Local preview: `npm install && npm run dev` or `just docs-dev`.

- **Handbook tab** — curated specs, ADRs, and guides under `docs/`
- **Code docs tab** — OpenWiki pages under `openwiki/` (refreshed through the on-demand workflow)

- [POC architecture](architecture/poc.md)
- [Actor-neutral capability map](architecture/capability-map.md)
- [Deployment boundary](architecture/deployment.md)
- [Observability seam](architecture/observability.md)
- [Operations HTTP API boundary](architecture/adr-operations-http-api.md)
- [Operations OpenAPI v1](https://github.com/RyanLisse/templiqx/blob/main/openapi/templiqx-operations-v1.yaml)
- [Contract format](contracts/v1alpha1.md)
- [Mock scenario format](contracts/mock-scenarios-v1alpha1.md)
- [CLI](guides/cli.md)
- [Operations HTTP API](guides/operations-api.md)
- [Generated client policy](guides/generated-clients.md)
- [SDK compatibility matrix](guides/compatibility.md)
- [Engine and SDK versioning](guides/versioning.md)
- [Pre-CRM3 readiness](guides/pre-crm3-readiness.md)
- [Host integration handoff](guides/host-integration.md)
- [Release procedure and artifact verification](guides/releasing.md)

Decisions (ADR):

- [Architecture decisions overview](adr/overview.md)
- [ADR: Package trust v1](adr/package-trust.md)
- [ADR: Tool-contract references](adr/tool-contract-refs.md)
- [ADR: Streaming RuntimeAdapter port](adr/streaming-runtime-port.md)
- [ADR: ODT compatibility](adr/odt-compatibility.md)

Agent skills:

- [Agent skills overview](skills/overview.md) — download and use the repo skills over MCP/CLI
- [Requirements](brainstorms/2026-07-11-templiqx-ai-native-template-engine-poc-requirements.md)
- [Implementation plan](plans/2026-07-11-templiqx-poc-implementation-plan.md)
- [Agent-native architecture audit (2026-07-12)](audits/2026-07-12-agent-native-architecture-review.md)
- [Agent-native architecture re-audit v2 (2026-07-13)](audits/2026-07-13-agent-native-architecture-review-v2.md)
- [Production release and conformance plan](plans/2026-07-13-001-feat-production-release-and-conformance-plan.md)
- [Deferred / host-blocked work log](plans/2026-07-13-deferred-work-log.md)
