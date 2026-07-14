# templiqx-bench

Deterministic, local-only benchmark harness for contract validation/compile and
document inspect/render paths. Produces a versioned JSON report suitable for
baseline comparison — no network, credentials, or cache state required.

```sh
cargo run -p templiqx-bench
cargo run -p templiqx-bench -- /path/to/templiqx
cargo test -p templiqx-bench
```

Report schema: `templiqx-bench/v1` with per-case median latency, functional
fingerprints, and output sizes. Hostile archive rejection is recorded separately
from successful render timing.
