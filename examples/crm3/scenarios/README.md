# Synthetic CRM3 Scenarios

These fixtures are conformance-only. They model CRM3-shaped inputs, diagnostics, stream expectations, and payload-free receipt policy without importing CRM3 runtime code or provider SDKs.

Inventory:

- `intake-document-01`: happy path extraction.
- `ambiguous-date`: ambiguous input rejected as invalid runtime response.
- `missing-notice-date`: missing required field rejected.
- `missing-required-field`: missing grounded notice date rejected permanently.
- `contradictory-evidence`: missing or contradictory evidence rejected permanently.
- `invalid-output-schema`: runtime receipt with invalid schema output.
- `docx-unresolved-reference`: drafting scenario with fingerprint-only receipt policy.

All manifests use `templiqx.mock/v1alpha1`; see `docs/contracts/mock-scenarios-v1alpha1.md`.
