use std::{
    env,
    error::Error,
    fmt,
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    path::PathBuf,
    time::Duration,
};

use axum::Router;
use templiqx_local::DeterministicFakeRuntime;
use templiqx_runtime_langfuse::{LangfuseConfig, LangfuseTracedRuntime, ModelConfig};
use tracing::info;

const DEFAULT_HTTP_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_MODEL_TIMEOUT_MS: u64 = 30_000;

type AnyError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ConfigError {}

struct Config {
    addr: SocketAddr,
    root: PathBuf,
    workspace: PathBuf,
    runtime: RuntimeConfig,
}

enum RuntimeConfig {
    DeterministicFake,
    Langfuse {
        model: ModelConfig,
        langfuse: LangfuseConfig,
    },
}

impl RuntimeConfig {
    const fn mode(&self) -> &'static str {
        match self {
            Self::DeterministicFake => "deterministic-fake",
            Self::Langfuse { .. } => "langfuse",
        }
    }
}

impl Config {
    fn from_env() -> Result<Self, AnyError> {
        let addr_source =
            env_value("TEMPLIQX_HTTP_ADDR").unwrap_or_else(|| DEFAULT_HTTP_ADDR.into());
        let addr = addr_source.parse().map_err(|error| {
            ConfigError(format!(
                "invalid TEMPLIQX_HTTP_ADDR '{addr_source}': {error}"
            ))
        })?;
        let root = PathBuf::from(env_value("TEMPLIQX_ROOT").unwrap_or_else(|| ".".into()));
        let workspace = env_value("TEMPLIQX_WORKSPACE")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join(".templiqx-workspace"));
        let runtime = RuntimeConfig::from_env()?;
        Ok(Self {
            addr,
            root,
            workspace,
            runtime,
        })
    }
}

impl RuntimeConfig {
    fn from_env() -> Result<Self, AnyError> {
        let Some(api_key) = env_value("MODEL_API_KEY") else {
            return Ok(Self::DeterministicFake);
        };
        let timeout_source =
            env_value("MODEL_TIMEOUT_MS").unwrap_or_else(|| DEFAULT_MODEL_TIMEOUT_MS.to_string());
        let timeout_ms = timeout_source.parse::<u64>().map_err(|error| {
            ConfigError(format!(
                "invalid MODEL_TIMEOUT_MS '{timeout_source}': {error}"
            ))
        })?;
        if timeout_ms == 0 {
            return Err(
                ConfigError("invalid MODEL_TIMEOUT_MS: must be greater than zero".into()).into(),
            );
        }
        Ok(Self::Langfuse {
            model: ModelConfig {
                base_url: required_env("MODEL_BASE_URL")?,
                api_key,
                model: required_env("MODEL_ID")?,
                timeout: Duration::from_millis(timeout_ms),
            },
            langfuse: LangfuseConfig {
                host: required_env("LANGFUSE_HOST")?,
                public_key: required_env("LANGFUSE_PUBLIC_KEY")?,
                secret_key: required_env("LANGFUSE_SECRET_KEY")?,
            },
        })
    }
}

fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn required_env(name: &str) -> Result<String, AnyError> {
    env_value(name)
        .ok_or_else(|| ConfigError(format!("missing required environment variable {name}")).into())
}

fn build_router(config: &Config) -> Result<Router, AnyError> {
    let router = match &config.runtime {
        RuntimeConfig::DeterministicFake => {
            let service = templiqx_local::compose_with_runtime(
                &config.root,
                &config.workspace,
                DeterministicFakeRuntime,
            )?;
            templiqx_http::router(service)
        }
        RuntimeConfig::Langfuse { model, langfuse } => {
            let runtime = LangfuseTracedRuntime::new(model.clone(), langfuse.clone()).map_err(
                |error| {
                    ConfigError(format!(
                        "invalid Langfuse runtime configuration from MODEL_BASE_URL, MODEL_ID, LANGFUSE_HOST, LANGFUSE_PUBLIC_KEY, or LANGFUSE_SECRET_KEY: {error}"
                    ))
                },
            )?;
            let service =
                templiqx_local::compose_with_runtime(&config.root, &config.workspace, runtime)?;
            templiqx_http::router(service)
        }
    };
    Ok(router)
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("templiqx-http-server: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), AnyError> {
    if env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--healthcheck")) {
        return healthcheck();
    }

    tracing_subscriber::fmt().with_target(false).init();
    let config = Config::from_env()?;
    let router = build_router(&config)?;
    let listener = tokio::net::TcpListener::bind(config.addr)
        .await
        .map_err(|error| {
            ConfigError(format!(
                "failed to bind TEMPLIQX_HTTP_ADDR {}: {error}",
                config.addr
            ))
        })?;
    let bound_addr = listener.local_addr()?;

    info!(
        bind_addr = %bound_addr,
        root = %config.root.display(),
        runtime = config.runtime.mode(),
        "templiqx HTTP server starting"
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    info!("templiqx HTTP server shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install SIGINT handler");
        "SIGINT"
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
        "SIGTERM"
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<&'static str>();

    let signal = tokio::select! {
        signal = ctrl_c => signal,
        signal = terminate => signal,
    };
    info!(
        signal,
        "shutdown signal received; draining in-flight requests"
    );
}

fn healthcheck() -> Result<(), AnyError> {
    let source = env_value("TEMPLIQX_HTTP_ADDR").unwrap_or_else(|| DEFAULT_HTTP_ADDR.into());
    let configured: SocketAddr = source.parse().map_err(|error| {
        ConfigError(format!(
            "invalid TEMPLIQX_HTTP_ADDR '{source}' for healthcheck: {error}"
        ))
    })?;
    let ip = match configured.ip() {
        IpAddr::V4(_) => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
    };
    let address = SocketAddr::new(ip, configured.port());
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let status = response.lines().next().unwrap_or_default();
    if !status.starts_with("HTTP/1.1 200 ") {
        return Err(ConfigError(format!("healthcheck failed: {status}")).into());
    }
    Ok(())
}
