use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::Value;
use std::{fs, path::PathBuf, process::ExitCode};
use templiqx_application::{MigrateLegacyRequest, RenderDocumentRequest};
use templiqx_contracts::{OperationEnvelope, RenderRequest};

#[derive(Parser)]
#[command(
    name = "templiqx",
    version,
    about = "Actor-neutral AI contract compiler"
)]
struct Cli {
    /// Directory containing portable package directories.
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    /// Emit compact JSON instead of pretty JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Catalog,
    Discover,
    Inspect {
        package: String,
        contract: String,
    },
    Put {
        package: String,
        contract: String,
        source: PathBuf,
        #[arg(long)]
        expected_fingerprint: Option<String>,
    },
    Validate {
        package: String,
        contract: Option<String>,
    },
    Compile {
        package: String,
        contract: String,
        #[arg(long)]
        values: Option<PathBuf>,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
    },
    Render {
        package: String,
        contract: String,
        #[arg(long)]
        values: Option<PathBuf>,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
    },
    Execute {
        package: String,
        contract: String,
        #[arg(long)]
        values: Option<PathBuf>,
        #[arg(long)]
        fixture_output: PathBuf,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
    },
    Test {
        package: String,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
    },
    Diff {
        left_package: String,
        left_contract: String,
        right_package: String,
        right_contract: String,
    },
    Explain {
        package: String,
        contract: String,
    },
    Migrate {
        package: String,
        dialect: String,
        /// Portable source path relative to the package root.
        source: String,
        #[arg(long)]
        aliases: Option<PathBuf>,
    },
    RenderDocument {
        package: String,
        /// Portable template path relative to the package root.
        template: String,
        data: PathBuf,
        /// Portable artifact path relative to the package root.
        output: String,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(ok) => {
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            }
        }
        Err(error) => {
            eprintln!("templiqx: {error:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    if matches!(cli.command, Command::Catalog) {
        let envelope = templiqx_application::catalog();
        return print(&envelope, cli.json);
    }
    let service = templiqx_local::compose(&cli.root).context("compose local Templiqx service")?;
    macro_rules! output {
        ($value:expr) => {{
            let envelope = $value;
            return print(&envelope, cli.json);
        }};
    }
    match cli.command {
        Command::Catalog => unreachable!(),
        Command::Discover => output!(service.discover_packages()),
        Command::Inspect { package, contract } => {
            output!(service.inspect_contract(&package, &contract))
        }
        Command::Put {
            package,
            contract,
            source,
            expected_fingerprint,
        } => {
            let source = fs::read_to_string(&source)
                .with_context(|| format!("read {}", source.display()))?;
            output!(service.put_contract(
                &package,
                &contract,
                &source,
                expected_fingerprint.as_deref()
            ));
        }
        Command::Validate { package, contract } => {
            if let Some(contract) = contract {
                output!(service.validate_contract(&package, &contract))
            } else {
                output!(service.validate_package(&package))
            }
        }
        Command::Compile {
            package,
            contract,
            values,
            capabilities,
        } => output!(service.compile_contract(
            &package,
            &contract,
            &read_request(values)?,
            &capabilities
        )),
        Command::Render {
            package,
            contract,
            values,
            capabilities,
        } => output!(service.render_contract(
            &package,
            &contract,
            &read_request(values)?,
            &capabilities
        )),
        Command::Execute {
            package,
            contract,
            values,
            fixture_output,
            capabilities,
        } => output!(service.execute_contract(
            &package,
            &contract,
            &read_request(values)?,
            &capabilities,
            Some(read_json(&fixture_output)?)
        )),
        Command::Test {
            package,
            capabilities,
        } => output!(service.test_package(&package, &capabilities)),
        Command::Diff {
            left_package,
            left_contract,
            right_package,
            right_contract,
        } => output!(service.diff_contract(
            &left_package,
            &left_contract,
            &right_package,
            &right_contract
        )),
        Command::Explain { package, contract } => {
            output!(service.explain_contract(&package, &contract))
        }
        Command::Migrate {
            package,
            dialect,
            source,
            aliases,
        } => {
            output!(service.migrate_legacy(
                &MigrateLegacyRequest {
                    package,
                    dialect,
                    source,
                    aliases:
                        aliases.map_or_else(
                            || Ok(Value::Object(Default::default())),
                            |p| read_json(&p)
                        )?
                }
            ))
        }
        Command::RenderDocument {
            package,
            template,
            data,
            output: artifact,
        } => output!(service.render_document(&RenderDocumentRequest {
            package,
            template,
            data: read_json(&data)?,
            output: artifact
        })),
    }
}

fn read_request(path: Option<PathBuf>) -> Result<RenderRequest> {
    path.map_or_else(
        || {
            Ok(RenderRequest {
                inputs: Default::default(),
                context: Default::default(),
            })
        },
        |p| {
            serde_json::from_value(read_json(&p)?)
                .with_context(|| format!("decode render request from {}", p.display()))
        },
    )
}
fn read_json(path: &PathBuf) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("decode JSON from {}", path.display()))
}
fn print<T: Serialize>(envelope: &OperationEnvelope<T>, compact: bool) -> Result<bool> {
    if compact {
        println!("{}", serde_json::to_string(envelope)?);
    } else {
        println!("{}", serde_json::to_string_pretty(envelope)?);
    }
    Ok(envelope.ok)
}
