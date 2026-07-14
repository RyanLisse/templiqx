# Verification report

Report:

1. Packages root and workspace root used.
2. Package, contract, and fixture IDs tested.
3. Operations invoked in order.
4. `ok` status for each envelope.
5. Stable diagnostic codes, source locations, and help text for failures.
6. Request, output, receipt, package, or artifact fingerprints returned.
7. Whether a repeated run produced matching fingerprints.
8. Whether the run used a mock adapter or a real provider adapter.

Never claim real-provider readiness from synthetic mock or conformance results.
