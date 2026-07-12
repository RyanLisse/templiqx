use std::{env, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| env::var_os("TEMPLIQX_PACKAGES_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let application = templiqx_local::compose(&root)?;
    templiqx_mcp::serve_stdio(templiqx_mcp::TempliqxMcp::new(application).with_packages_root(root))
        .await
}
