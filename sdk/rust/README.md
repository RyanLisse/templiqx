# Templiqx Rust adapter pilot

Thin async Rust client for the Templiqx Operations HTTP API. The wire contract
owns validation and semantics; this crate only handles URLs, headers, timeouts,
JSON DTOs, and transport errors. It is intentionally outside the server Cargo
workspace and imports no Templiqx server crate.

```toml
[dependencies]
templiqx-adapter-rust = { path = "../templiqx/sdk/rust" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust,no_run
use templiqx_adapter_rust::{CallOptions, Client, ClientOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new("http://127.0.0.1:8080", ClientOptions::default())?;
    let catalog = client.catalog(CallOptions::default()).await?;
    println!("{:?}", catalog.data.result);
    Ok(())
}
```

Every call accepts a caller request ID or generates a UUID, and supports a
per-call timeout. Required compare-and-swap methods take `CasCallOptions` and
send its value as `If-Match`. Dropping an in-flight future cancels the request.
There are no retries or client-side contract validation.

DTOs are generated with pinned, dev-only Typify tooling from
`../../openapi/templiqx-operations-v1.yaml` and checked in:

```sh
./scripts/generate.sh
./scripts/check-generated.sh
cargo clippy --all-targets -- -D warnings
cargo test
TEMPLIQX_SDK_IT=1 cargo test -- --ignored --nocapture
```
