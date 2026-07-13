use std::{env, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| env::var_os("TEMPLIQX_PACKAGES_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let workspace = env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .or_else(|| env::var_os("TEMPLIQX_WORKSPACE_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| root.join(".templiqx-workspace"));
    let application = templiqx_local::compose_with_workspace(&root, &workspace)?;
    templiqx_mcp::serve_stdio(
        templiqx_mcp::TempliqxMcp::new(application)
            .with_packages_root(root)
            .with_workspace_root(workspace),
    )
    .await
}
