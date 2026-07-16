---
title: Report engine compatibility
---

This guide maps legacy v5 report "powers" to what Templiqx proves today for the
BLI-230 report-engine PoC. It distinguishes **measured** support (fixtures +
conformance/benches) from **non-claims** and host-owned prerequisites.

## Legacy corpus: format evidence, not dialect fidelity

The files under `examples/legacy-corpus/v5-report-templates/` evidence **which wire formats still
appear in the wild**. They are **not** frozen Templiqx definitions and do **not**
prove dialect fidelity for Velocity / MERGEFIELD / `$func` / cell scripts.

| Artifact | What it evidences | What it does **not** mean |
|----------|-------------------|---------------------------|
| `Dossiervoorblad.docx` | DOCX still used for dossier covers | Not a simple modern cover — dense Velocity-in-MERGEFIELD authoring. Templiqx does **not** execute Velocity; proof is via bounded `docx-v5` + synthetic report definitions |
| `Bedrijfsoverzicht.rtf` | Real RTF report usage (**gate for building `templiqx-rtf`**) | Not a simple memo — heavy tables, `#if`, `$func`. Adapter emits bounded interpolated RTF; it does **not** parse or run Velocity/`$func` |
| `OHW-per-kostenplaats.xls` | Spreadsheet reports still exist as BIFF `.xls` | **Input non-claim** — no BIFF reader. Emit `.xlsx`/CSV/XML from frozen tabular bindings; aggregation stays in the host/query layer |

Do **not** treat these binaries as migration fixtures that Templiqx must round-trip.
Semantic successors (if needed later) are new report-definition + merge_data packages,
not Velocity interpreters.

## v5 four powers → Templiqx status

| v5 power | Status | Templiqx proof |
|----------|--------|----------------|
| Report definition / authoring | covered | [`report-definition-v1alpha1`](../contracts/report-definition-v1alpha1.md) + `examples/packages/basenet-legal/definitions/dunning-letter-v1.yaml` |
| Authorized data query | partial | `DataIntrospectPort` / `AuthorizedQueryPort` traits + `fixtures/authorized-query-response.json` + local fake — **no query execution in core** |
| Merge + render | covered | `merge-data-v1alpha1` `customFields`, evidence fragments, `docx-v5` render, determinism + fan-out benches |
| Multi-format output | covered | DOCX/HTML/Typst/XLSX/CSV/XML/Markdown/RTF adapters; PDF via host conversion seam |

## Format support matrix

| Format | Status | Notes |
|--------|--------|-------|
| DOCX | covered | `templiqx-docx-v5` |
| HTML / plain | covered | `templiqx-html-plain` |
| PDF | host seam | Recorded-fixture manifests only; converter is host-owned ([document-conversion ADR](../adr/document-conversion.md)) |
| Typst | covered | `templiqx-typst` emits deterministic markup; PDF compile stays host-owned |
| XLSX | covered | `templiqx-xlsx` (`rust_xlsxwriter`) with native column charts |
| CSV / XML | covered | `templiqx-tabular` thin serializers (not v5 report-XML) |
| Markdown | covered | `templiqx-markdown` (`markdown-rs`) → safe HTML/plain; **no MDX** |
| RTF | covered | `templiqx-rtf` hand-rolled emitter — format evidenced by `Bedrijfsoverzicht.rtf`; **not** Velocity/`$func` fidelity |
| Legacy binary `.xls` **input** | non-claim | No BIFF reader; Templiqx emits `.xlsx`/CSV/XML from frozen tabular data only |

## Format scope decision (U10)

ADR-0019 listed DOCX/RTF/XLS/XLSX and dropped XML/CSV; v5 had eight formats.
Templiqx now covers:

**DOCX · HTML/plain · PDF (host seam) · Typst · XLSX · CSV · XML · Markdown · RTF**

Explicit non-claims:

- no OData / reflective / `$apply` **query execution** in portable core (traits + fixtures only)
- no retrieval / DMS / OCR ownership
- no approval workflow state machine (approval fields on report definitions are metadata only)
- no v5 report-XML output (accounting Exact/Twinfield XML is a separate connector)
- no standalone chart engine (charts are native to Typst markup and `rust_xlsxwriter`)
- no legacy binary `.xls` (BIFF) **input** reader
- no Velocity / MERGEFIELD-script / `$func` / cell-script **execution** — adapters interpolate frozen definitions + approved `merge_data` only
- no aggregation engine in portable core (billing/OHW math stays host/query-owned)

## Receipt fingerprint == document-store checksum (R10)

A generated report's Templiqx receipt fingerprint is a SHA-256 over artifact
bytes. That value **is** the host `document_version.checksum`: one integrity
concept for uploaded and generated documents, one row per version, **no**
separate report-receipt table (BLI-68). Persistence remains host-owned.

## Host prerequisites (R12 — tracked, host-built)

Templiqx ships ports and guardrails; the host must still build:

1. **`compileToFilter(policy, actor, resourceType)` (ADR-0002, unbuilt)** — row-level
   `can()` enforcement every authorized query needs. Highest-priority host prerequisite.
2. **`document_version` write race** — version computed before unique insert; fix before
   Templiqx receipts land as `document_version` rows.
3. **AI authoring agent + hybrid loop + A/B routing** — host component; Templiqx supplies
   `validate` / `compile` / `explain` / `diff` only.
4. **Query interface choice** (OData vs GraphQL vs DSL) — resolve before the query port hardens.

See [Host integration](host-integration.md) for assembler handoff details.
