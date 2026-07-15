---
title: "ADR: Document conversion (PDF and host-owned converters)"
---

## Status

Accepted (2026-07-15) — host-owned conversion seam and recorded conformance
fixture prove entry criteria; no default-composition converter ships in this
repository.

## Context

Document-template workflows often require PDF or other rendered formats. Templiqx
ships a measured DOCX V5 inspect/render slice and actor-neutral
`inspect_document`, but does not own subprocess converters, font discovery,
retry queues, or tenant quotas.

## Decision

1. **PDF conversion is a host-constructed optional adapter**, not default
   composition. The portable core must not probe host installations or spawn
   converters.
2. **Entry criteria before implementation:**
   - dedicated ADR acceptance (this record);
   - converter identity and environment reporting in render receipts;
   - controlled subprocess tests with synthetic fixtures;
   - corpus-backed parity or explicit approximated/unsupported categories.
3. **Queue, retry, quota, and process-isolation policy remain host-owned.**

## Consequences

- CRM3 and repository conformance stay DOCX-grounded without PDF claims.
- Hosts may wrap LibreOffice, Gotenberg, or opco-specific converters behind
  `DocumentRenderer` or a separate conversion port constructed explicitly.

## Alternatives considered

- **Default CLI/MCP PDF renderer.** Rejected — pulls host policy and
  non-deterministic subprocess behavior into the product path.
- **Core-owned converter daemon.** Rejected — violates boundary checks and
  couples portable core to host runtime.
