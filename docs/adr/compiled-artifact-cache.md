# ADR: Compiled-artifact cache

## Status

Proposed (2026-07-14) — gated on benchmark evidence; not implemented.

## Context

Contract compilation is stateless today. Before adding cache ports or mutable
core state, the project needs reproducible baselines for compile/render and
document inspect/render latency, output size, and hostile-input rejection cost.

## Decision

1. **No cache in the first safe document-template wave.** Unit 3 benchmark
   harness (`tools/templiqx-bench`) records machine-readable baselines locally.
2. **A cache is considered only when:**
   - benchmarks identify a material, repeatable bottleneck;
   - keys include package/contract/input/context/capability/compiler
     fingerprints relevant to the cached artifact;
   - storage is host/store-backed outside `templiqx-core` and default local
     composition.
3. **Package identity and receipts must remain valid without cache hits.**

## Consequences

- Hosts may implement their own caches behind typed ports without changing
  portable contracts.
- Performance claims require cited benchmark reports, not assumed speedups.

## Alternatives considered

- **In-core memoization.** Rejected — breaks determinism proofs and boundary
  rules for default composition.
- **Implicit HTTP cache headers only.** Rejected — insufficient for compile
  artifacts and document render receipts.
