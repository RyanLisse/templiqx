use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};
use templiqx_application::{MigrateLegacyRequest, RenderDocumentRequest};
use templiqx_contracts::{OperationEnvelope, RenderRequest, fingerprint, fingerprint_bytes};

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
        /// Portable artifact path relative to the workspace root.
        output: String,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Run the synthetic CRM3 conformance workload used by Docker and kind smoke.
    Crm3Conformance {
        #[arg(long, default_value = "crm3")]
        package: String,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        receipt: Option<PathBuf>,
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
    if let Command::Catalog = &cli.command {
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
            workspace,
        } => output!(service.render_document(&RenderDocumentRequest {
            package,
            template,
            data: read_json(&data)?,
            output: artifact,
            workspace: workspace_string(workspace)?,
        })),
        Command::Crm3Conformance {
            package,
            workspace,
            receipt,
        } => {
            let report = crm3_conformance(&service, &cli.root, &package, workspace)?;
            if let Some(path) = receipt {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("create {}", parent.display()))?;
                }
                fs::write(&path, serde_json::to_vec_pretty(&report)?)
                    .with_context(|| format!("write {}", path.display()))?;
            }
            println!(
                "{}",
                if cli.json {
                    serde_json::to_string(&report)?
                } else {
                    serde_json::to_string_pretty(&report)?
                }
            );
            Ok(true)
        }
    }
}

fn crm3_conformance(
    service: &templiqx_local::LocalService,
    root: &Path,
    package: &str,
    workspace: PathBuf,
) -> Result<Value> {
    let package_root = root.join(package);
    let capabilities = vec!["structured_output".to_owned()];
    let extraction_request = read_request(Some(package_root.join("evals/bli-61-request.json")))?;
    let extraction_output = read_json(&package_root.join("evals/bli-61-output.json"))?;
    let extraction = service.execute_contract(
        package,
        "bli-61-date-term-extraction",
        &extraction_request,
        &capabilities,
        Some(extraction_output.clone()),
    );
    ensure!(
        extraction.ok,
        "extraction failed: {:?}",
        extraction.diagnostics
    );
    let extraction_receipt = extraction.result.context("extraction receipt")?;

    let mut drafting_request = read_request(Some(package_root.join("evals/bli-62-request.json")))?;
    drafting_request
        .inputs
        .insert("extracted_facts".into(), extraction_output);
    let drafting_output = read_json(&package_root.join("evals/bli-62-output.json"))?;
    let drafting = service.execute_contract(
        package,
        "bli-62-document-drafting",
        &drafting_request,
        &capabilities,
        Some(drafting_output.clone()),
    );
    ensure!(drafting.ok, "drafting failed: {:?}", drafting.diagnostics);
    let drafting_receipt = drafting.result.context("drafting receipt")?;

    let rendered = service.render_document(&RenderDocumentRequest {
        package: package.into(),
        template: "templates/v5-contract-template.docx".into(),
        data: drafting_output["merge_data"].clone(),
        output: "crm3-conformance/rendered.docx".into(),
        workspace: Some(workspace.to_string_lossy().into_owned()),
    });
    ensure!(
        rendered.ok,
        "document render failed: {:?}",
        rendered.diagnostics
    );
    let rendered = rendered.result.context("rendered document")?;
    let artifact = workspace.join(package).join(&rendered.artifact);
    let artifact_bytes =
        fs::read(&artifact).with_context(|| format!("read {}", artifact.display()))?;
    let artifact_fingerprint = fingerprint_bytes(&artifact_bytes);
    let report = json!({
        "api_version": "templiqx/conformance/v1",
        "package": package,
        "workspace_artifact": rendered.artifact,
        "extraction": {
            "request_fingerprint": extraction_receipt.request_fingerprint,
            "output_fingerprint": extraction_receipt.output_fingerprint,
            "schema_valid": extraction_receipt.output_schema_valid
        },
        "drafting": {
            "request_fingerprint": drafting_receipt.request_fingerprint,
            "output_fingerprint": drafting_receipt.output_fingerprint,
            "schema_valid": drafting_receipt.output_schema_valid
        },
        "document": {
            "artifact_fingerprint": artifact_fingerprint,
            "report_fingerprint": fingerprint(&rendered.report)?
        }
    });
    Ok(report)
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
fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("decode JSON from {}", path.display()))
}
fn workspace_string(path: Option<PathBuf>) -> Result<Option<String>> {
    path.map(|path| {
        let absolute = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
        Ok(absolute.to_string_lossy().into_owned())
    })
    .transpose()
}
fn print<T: Serialize>(envelope: &OperationEnvelope<T>, compact: bool) -> Result<bool> {
    if compact {
        println!("{}", serde_json::to_string(envelope)?);
    } else {
        println!("{}", serde_json::to_string_pretty(envelope)?);
    }
    Ok(envelope.ok)
}
