---
title: "ADR: Legacy template import (safe subset)"
---

## Status

Proposed (2026-07-14) — explicit unsupported classification; no broad importer.

## Context

Estates may carry Handlebars, Carbone, or other legacy template dialects.
Templiqx already migrates an explicit DOCX V5 subset via `LegacyImportAdapter`
with fixture-gated claims. Broader legacy import risks executable helpers,
dynamic partials, and silent approximation.

## Decision

1. **Only explicitly selected dialects are imported.** V5 DOCX is the measured
   slice; V1 BeanShell and V2 markers are detected and classified, never
   executed.
2. **Unsupported by design in portable core:**
   - arbitrary JavaScript, Rust, shell, BeanShell, Handlebars helpers/decorators;
   - dynamic partial lookup and prototype traversal;
   - unbounded inheritance or runtime helper registries.
3. **Future dialects require:** fixture IDs, expected migration reports, and ADR
   acceptance before catalog claims.

## Consequences

- `migrate_legacy` remains the host-facing import seam; hosts add adapters for
  opco-specific subsets with their own corpora.
- Named slots, YAML LSP, and Rust authoring facades are separate proposals with
  prerequisites — not hidden extensions to v1alpha1 syntax.

## Alternatives considered

- **Generic Handlebars compatibility layer.** Rejected — executable helper
  surface conflicts with non-executable contract model.
- **Silent best-effort import.** Rejected — fixture discipline requires honest
  category reporting.
