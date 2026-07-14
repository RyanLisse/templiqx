use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};
use templiqx_application::{
    CreatePackageRequest, DeleteContractRequest, DeletePackageRequest,
    DeleteWorkspaceArtifactRequest, ListWorkspaceArtifactsRequest, MigrateLegacyRequest,
    ReadArtifactRequest, RenderDocumentRequest, SignPackageRequest, UpdatePackageRequest,
    VerifyPackageTrustRequest,
};
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
    Create {
        name: String,
        #[arg(long, default_value = "0.1.0")]
        version: String,
    },
    UpdatePackage {
        package: String,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        expected_fingerprint: String,
    },
    DeletePackage {
        package: String,
        #[arg(long)]
        expected_fingerprint: String,
    },
    ExportPackageIdentity {
        package: String,
    },
    SignPackage {
        package: String,
        #[arg(long)]
        key_id: String,
        #[arg(long)]
        expected_fingerprint: String,
    },
    VerifyPackageTrust {
        package: String,
        #[arg(long)]
        strict: bool,
    },
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
    Delete {
        package: String,
        contract: String,
        #[arg(long)]
        expected_fingerprint: String,
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
        /// Drive the streaming runtime path; the receipt is identical to the
        /// non-streaming path (fingerprint parity).
        #[arg(long)]
        stream: bool,
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
    ListWorkspaceArtifacts {
        package: String,
        #[arg(long)]
        workspace: Option<PathBuf>,
        #[arg(long)]
        prefix: Option<String>,
    },
    ReadArtifact {
        package: String,
        path: String,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    DeleteWorkspaceArtifact {
        package: String,
        path: String,
        #[arg(long)]
        workspace: Option<PathBuf>,
        #[arg(long)]
        expected_fingerprint: String,
    },
    ListEvals {
        package: String,
    },
    RunEval {
        package: String,
        contract: String,
        fixture_id: String,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
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
        Command::Create { name, version } => {
            output!(service.create_package(&CreatePackageRequest { name, version }))
        }
        Command::UpdatePackage {
            package,
            version,
            description,
            expected_fingerprint,
        } => output!(service.update_package(&UpdatePackageRequest {
            package,
            version,
            description,
            expected_fingerprint,
        })),
        Command::DeletePackage {
            package,
            expected_fingerprint,
        } => output!(service.delete_package(&DeletePackageRequest {
            package,
            expected_fingerprint,
        })),
        Command::ExportPackageIdentity { package } => {
            output!(service.export_package_identity(&package))
        }
        Command::SignPackage {
            package,
            key_id,
            expected_fingerprint,
        } => output!(service.sign_package(&SignPackageRequest {
            package,
            key_id,
            expected_fingerprint,
        })),
        Command::VerifyPackageTrust { package, strict } => {
            output!(service.verify_package_trust(&VerifyPackageTrustRequest { package, strict }))
        }
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
        Command::Delete {
            package,
            contract,
            expected_fingerprint,
        } => output!(service.delete_contract(&DeleteContractRequest {
            package,
            contract,
            expected_fingerprint,
        })),
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
            stream,
        } => output!(service.execute_contract(
            &package,
            &contract,
            &read_request(values)?,
            &capabilities,
            Some(read_json(&fixture_output)?),
            stream
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
        Command::ListWorkspaceArtifacts {
            package,
            workspace,
            prefix,
        } => output!(
            service.list_workspace_artifacts(&ListWorkspaceArtifactsRequest {
                package,
                workspace: workspace_string(workspace)?,
                prefix,
            })
        ),
        Command::ReadArtifact {
            package,
            path,
            workspace,
        } => output!(service.read_artifact(&ReadArtifactRequest {
            package,
            path,
            workspace: workspace_string(workspace)?,
        })),
        Command::DeleteWorkspaceArtifact {
            package,
            path,
            workspace,
            expected_fingerprint,
        } => output!(
            service.delete_workspace_artifact(&DeleteWorkspaceArtifactRequest {
                package,
                path,
                workspace: workspace_string(workspace)?,
                expected_fingerprint,
            })
        ),
        Command::ListEvals { package } => output!(service.list_evals(&package)),
        Command::RunEval {
            package,
            contract,
            fixture_id,
            capabilities,
        } => output!(service.run_eval(&package, &contract, &fixture_id, &capabilities)),
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
        false,
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
        false,
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
