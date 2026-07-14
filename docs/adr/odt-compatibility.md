# ADR: ODT (OpenDocument Text) compatibility

## Status

Accepted (2026-07-13) — design + detect-only migration; no render adapter in this slice.

## Context

Templiqx renders one narrow, measured DOCX V5 subset (`adapters/templiqx-docx-v5`)
for the CRM3 fixture class. Some opco document estates carry OpenDocument Text
(`.odt`) sources instead of, or alongside, `.docx`. Plan 001 U8 and brainstorm
R12/R24 name ODT as a portability question: can a package that ships `.odt`
templates be discovered and reported without silently mis-migrating it, and
without Templiqx claiming render parity it has not proven?

The project's fixture discipline is explicit: no compatibility claim ships
without a synthetic fixture and a migration report that categorizes each field
(`migrated`, `approximated`, `unsupported`, `unsafe`). ODT rendering would
require a second OOXML-equivalent canonicalization and parity baseline — a
large surface with no CRM3 use case today.

## Decision

1. **Detect-only in migration; no ODT `DocumentRenderer` in this slice.**
   The legacy import path recognizes `.odt` by ZIP + `mimetype` entry
   (`application/vnd.oasis.opendocument.text`) and emits a migration report
   with every content field categorized `unsupported` and a single
   `TQX_ODT_DETECTED` diagnostic. No field is executed, migrated, or rendered.
   This mirrors the existing V1 BeanShell / V2 posture: detected, reported,
   never run.

2. **No render adapter, no parity claim.** Templiqx does not add an
   `templiqx-odt` render crate here. The DOCX V5 adapter stays the only
   document renderer in the default composition. An ODT render adapter, if ever
   built, follows the same fixture-first rule: synthetic `.odt` fixtures with
   expected OOXML/ODT-normalized parity baselines before any "supported" claim.

3. **Boundary posture unchanged.** ODT detection lives in the legacy-import
   adapter surface, not in `templiqx-contracts`/`templiqx-core`. The portable
   core gains no ODT vocabulary. `scripts/check-boundaries.sh` continues to pass.

4. **Reporting shape reuses `MigrationResult`.** No new report type. An ODT
   source yields a `MigrationResult` whose categories are all `unsupported`,
   with the `TQX_ODT_DETECTED` diagnostic explaining that ODT is recognized but
   not migrated in this version — an honest, machine-readable "we saw it, we
   will not pretend to render it."

## Consequences

- A package containing `.odt` sources fails closed at migration with a clear,
  stable diagnostic instead of producing a wrong or partial document.
- The compatibility matrix in `adapters/templiqx-docx-v5/README.md` and the
  legacy corpus README can list ODT as `detected, not migrated` — a measured
  scope statement, not a parity claim.
- Building actual ODT render later is unblocked but gated behind the same
  fixture + parity discipline the DOCX slice already follows.

## Alternatives considered

- **Full ODT render adapter now.** Rejected — no CRM3 fixture needs it, and a
  second canonicalization/parity surface is disproportionate to current demand.
- **Silently ignore `.odt` (treat as unknown binary).** Rejected — silent
  handling hides a real input class; detect-and-report is the honest posture
  and matches how V1/V2 legacy dialects are already surfaced.
- **Best-effort ODT→DOCX conversion via a third-party library.** Rejected —
  pulls a heavy non-deterministic dependency into the adapter surface and would
  produce approximated output the project cannot back with a parity baseline.

## Open questions

- Whether ODT detection should distinguish OpenDocument *Text* from other ODF
  types (spreadsheet/presentation) — deferred; only `.odt` text is in scope if
  render is ever added.
- Whether a future ODT adapter shares the DOCX canonicalizer or needs its own —
  a P2+ implementation question, not blocking this detect-only decision.
