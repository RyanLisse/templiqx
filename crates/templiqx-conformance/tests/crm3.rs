use anyhow::{Context, Result, anyhow, ensure};
use rmcp::{
    ServiceExt as _,
    model::{CallToolRequestParams, JsonObject},
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use templiqx_application::{MigrateLegacyRequest, RenderDocumentRequest};
use templiqx_conformance::{
    ConformanceTraceReceipt, DocumentEvidence, InteractionEvidence, TRACE_API_VERSION,
    file_fingerprint, report_fingerprint,
};
use templiqx_contracts::{Contract, ExecutionReceipt, OperationEnvelope, RenderRequest};
use templiqx_docx_v5::DocxV5Adapter;
use templiqx_mcp::TempliqxMcp;
use templiqx_ports::{LegacyImportAdapter, LegacyImportRequest, LegacyImportResult};

const PACKAGE: &str = "crm3";
const EXTRACTION: &str = "bli-61-date-term-extraction";
const DRAFTING: &str = "bli-62-document-drafting";
const CAPABILITIES: &[&str] = &["structured_output"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn read_json(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn request(path: impl AsRef<Path>) -> Result<RenderRequest> {
    Ok(serde_json::from_value(read_json(path)?)?)
}

fn result<T>(envelope: OperationEnvelope<T>) -> Result<T> {
    ensure!(
        envelope.ok,
        "{} failed: {:?}",
        envelope.operation,
        envelope.diagnostics
    );
    envelope
        .result
        .ok_or_else(|| anyhow!("{} returned no result", envelope.operation))
}

fn fingerprint<T: Serialize>(value: &T) -> Result<String> {
    Ok(templiqx_contracts::fingerprint(value)?)
}

fn interaction_evidence(
    contract: &Contract,
    contract_fingerprint: String,
    compiled_fingerprint: String,
    request: &RenderRequest,
    target_capabilities: &[String],
    receipt: &ExecutionReceipt,
) -> Result<InteractionEvidence> {
    Ok(InteractionEvidence {
        contract_id: contract.id.clone(),
        contract_version: contract.version.clone(),
        contract_fingerprint,
        compiled_fingerprint,
        input_fingerprint: fingerprint(&request.inputs)?,
        context_fingerprint: fingerprint(&request.context)?,
        target_capability_profile_fingerprint: fingerprint(&target_capabilities.to_vec())?,
        adapter_id: receipt.adapter.id.clone(),
        adapter_version: receipt.adapter.version.clone(),
        request_fingerprint: receipt.request_fingerprint.clone(),
        output_fingerprint: receipt.output_fingerprint.clone(),
        output_schema_fingerprint: fingerprint(&contract.output_schema)?,
        output_schema_valid: receipt.output_schema_valid,
    })
}

fn assert_grounded_evidence(request: &RenderRequest, output: &Value) -> Result<()> {
    let source = request.inputs["document_text"]
        .as_str()
        .context("document_text")?;
    let document_id = request.inputs["document_id"]
        .as_str()
        .context("document_id")?;
    let fragment_id = request.inputs["fragment_id"]
        .as_str()
        .context("fragment_id")?;
    let evidence = output["evidence"].as_array().context("evidence")?;
    ensure!(evidence.len() == 4);
    for item in evidence {
        ensure!(item["document_id"] == document_id);
        ensure!(item["fragment_id"] == fragment_id);
        let start = usize::try_from(item["start_offset"].as_u64().context("start")?)?;
        let end = usize::try_from(item["end_offset"].as_u64().context("end")?)?;
        ensure!(start < end && end <= source.len());
        ensure!(source.is_char_boundary(start) && source.is_char_boundary(end));
        let quote = &source.as_bytes()[start..end];
        ensure!(hex::encode(Sha256::digest(quote)) == item["quote_sha256"]);
    }
    Ok(())
}

#[test]
fn composes_grounded_interactions_and_explicit_v5_document_conformance() -> Result<()> {
    let root = repo_root();
    let examples = root.join("examples");
    let package = examples.join(PACKAGE);
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(&examples, workspace.path())?;
    let capabilities = CAPABILITIES
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let manifest = result(service.discover_packages())?
        .into_iter()
        .find(|manifest| manifest.package == PACKAGE)
        .context("CRM3 manifest")?;
    let package_validation = service.validate_package(PACKAGE);
    ensure!(
        package_validation.ok,
        "package diagnostics: {:?}",
        package_validation.diagnostics
    );
    let package_fingerprint = package_validation.fingerprints["package"].clone();
    let eval_report = result(service.test_package(PACKAGE, &capabilities))?;
    ensure!(eval_report.passed == 2 && eval_report.failed == 0);

    let extraction_contract = result(service.inspect_contract(PACKAGE, EXTRACTION))?;
    let extraction_validation = service.validate_contract(PACKAGE, EXTRACTION);
    ensure!(extraction_validation.ok);
    let extraction_contract_fingerprint = extraction_validation.fingerprints["contract"].clone();
    let extraction_request = request(package.join("evals/bli-61-request.json"))?;
    let extraction_compile =
        service.compile_contract(PACKAGE, EXTRACTION, &extraction_request, &capabilities);
    ensure!(extraction_compile.ok);
    let extraction_compiled_fingerprint =
        extraction_compile.fingerprints["compiled_interaction"].clone();
    let extraction_receipt = result(service.execute_contract(
        PACKAGE,
        EXTRACTION,
        &extraction_request,
        &capabilities,
        Some(read_json(package.join("evals/bli-61-output.json"))?),
        false,
    ))?;
    ensure!(extraction_receipt.output_schema_valid);
    assert_grounded_evidence(&extraction_request, &extraction_receipt.output)?;

    // The drafting input is exactly the schema-valid extraction output. In
    // particular, notice_date is grounded by BLI-61 rather than invented here.
    let mut drafting_request = request(package.join("evals/bli-62-request.json"))?;
    drafting_request
        .inputs
        .insert("extracted_facts".into(), extraction_receipt.output.clone());
    ensure!(
        drafting_request.inputs["extracted_facts"]["notice_date"]
            == extraction_receipt.output["notice_date"]
    );
    let drafting_contract = result(service.inspect_contract(PACKAGE, DRAFTING))?;
    let drafting_validation = service.validate_contract(PACKAGE, DRAFTING);
    ensure!(drafting_validation.ok);
    let drafting_contract_fingerprint = drafting_validation.fingerprints["contract"].clone();
    let drafting_compile =
        service.compile_contract(PACKAGE, DRAFTING, &drafting_request, &capabilities);
    ensure!(drafting_compile.ok);
    let drafting_compiled_fingerprint =
        drafting_compile.fingerprints["compiled_interaction"].clone();
    let drafting_receipt = result(service.execute_contract(
        PACKAGE,
        DRAFTING,
        &drafting_request,
        &capabilities,
        Some(read_json(package.join("evals/bli-62-output.json"))?),
        false,
    ))?;
    ensure!(drafting_receipt.output_schema_valid);

    let temp = tempfile::Builder::new()
        .prefix(".templiqx-conformance-")
        .tempdir_in(&package)?;
    let template = temp.path().join("v5-contract-template.docx");
    fs::copy(
        package.join("templates/v5-contract-template.docx"),
        &template,
    )?;
    let aliases = read_json(package.join("migrations/v5-aliases.json"))?;
    let adapter = DocxV5Adapter::default();
    let analyzed = adapter.analyze(&template, &aliases)?;
    ensure!(analyzed.findings.iter().any(|finding| {
        finding.reference.as_deref() == Some("client.name") && finding.detail.contains("alias")
    }));
    ensure!(
        analyzed
            .findings
            .iter()
            .filter(|finding| {
                finding.reference.as_deref() == Some("client.name")
                    && finding.detail.contains("alias")
            })
            .count()
            == 2
    );
    ensure!(
        analyzed
            .findings
            .iter()
            .filter(|finding| finding.construct == "mergefield")
            .count()
            == 2
    );
    ensure!(
        analyzed
            .findings
            .iter()
            .filter(|finding| finding.construct == "v5_reference")
            .count()
            == 6
    );
    for required_part in ["word/document.xml", "word/header1.xml", "word/footer1.xml"] {
        ensure!(
            analyzed
                .findings
                .iter()
                .any(|finding| finding.part == required_part)
        );
    }

    let LegacyImportResult {
        report: migration_report,
        canonical_template,
    } = LegacyImportAdapter::migrate(
        &adapter,
        &LegacyImportRequest {
            dialect: "v5".into(),
            source: template.clone(),
            aliases,
        },
    )?;
    let migrated = canonical_template.context("V5 migration must produce a template")?;
    let artifact = temp.path().join(PACKAGE).join("rendered.docx");
    let merge_data = drafting_receipt.output["merge_data"].clone();
    let rendered = result(service.render_document(&RenderDocumentRequest {
        package: PACKAGE.into(),
        template: package_relative(&package, &migrated)?,
        data: merge_data.clone(),
        output: "rendered.docx".into(),
        workspace: Some(temp.path().to_string_lossy().into_owned()),
    }))?;
    let render_report = rendered.report;
    ensure!(render_report["replacements"] == 7);
    ensure!(
        render_report["unresolved"]
            .as_array()
            .is_some_and(|items| items.len() == 1)
    );
    ensure!(render_report["unresolved"][0]["reference"] == "missing.reference");

    let baseline = package.join("baselines/v5-contract-approved.docx");
    let parity = adapter.compare_normalized(&artifact, &baseline)?;
    ensure!(parity.equal, "OOXML parity report: {parity:?}");
    for required_part in ["word/document.xml", "word/header1.xml", "word/footer1.xml"] {
        ensure!(
            parity
                .compared_parts
                .iter()
                .any(|part| part.part == required_part && part.equal)
        );
    }

    let trace = ConformanceTraceReceipt {
        api_version: TRACE_API_VERSION.into(),
        package: PACKAGE.into(),
        package_version: manifest.version,
        package_fingerprint,
        eval_report_fingerprint: fingerprint(&eval_report)?,
        interactions: vec![
            interaction_evidence(
                &extraction_contract,
                extraction_contract_fingerprint,
                extraction_compiled_fingerprint,
                &extraction_request,
                &capabilities,
                &extraction_receipt,
            )?,
            interaction_evidence(
                &drafting_contract,
                drafting_contract_fingerprint,
                drafting_compiled_fingerprint,
                &drafting_request,
                &capabilities,
                &drafting_receipt,
            )?,
        ],
        document: DocumentEvidence {
            adapter_id: "templiqx-docx-v5".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_template_fingerprint: file_fingerprint(&template)?,
            canonical_template_fingerprint: file_fingerprint(&migrated)?,
            migration_report_fingerprint: report_fingerprint(&migration_report)?,
            render_input_fingerprint: fingerprint(&merge_data)?,
            render_report_fingerprint: report_fingerprint(&render_report)?,
            artifact_fingerprint: file_fingerprint(&artifact)?,
            approved_baseline_fingerprint: file_fingerprint(&baseline)?,
            parity_report_fingerprint: fingerprint(&parity)?,
            normalized_ooxml_equal: parity.equal,
            unresolved_references: 1,
        },
    };
    let trace_json = serde_json::to_string_pretty(&trace)?;
    for forbidden_payload in [
        "Voorbeeldhandel",
        "De overeenkomst",
        "Concept contractsamenvatting",
        "merge_data",
        "31 mei 2027",
    ] {
        ensure!(!trace_json.contains(forbidden_payload));
    }
    ensure!(
        trace
            .interactions
            .iter()
            .all(|item| item.output_schema_valid)
    );
    ensure!(trace.document.normalized_ooxml_equal);
    Ok(())
}

fn cli_envelope(root: &Path, args: &[&str]) -> Result<Value> {
    static BUILT: OnceLock<()> = OnceLock::new();
    let repo = repo_root();
    if BUILT.get().is_none() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let build = Command::new(&cargo)
            .current_dir(&repo)
            .args(["build", "--quiet", "-p", "templiqx-cli"])
            .status()?;
        ensure!(build.success(), "failed to build templiqx CLI");
        let _ = BUILT.set(());
    }
    let binary = repo.join("target/debug").join(if cfg!(windows) {
        "templiqx.exe"
    } else {
        "templiqx"
    });
    let output = Command::new(binary)
        .arg("--root")
        .arg(root)
        .arg("--json")
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn arguments(value: Value) -> JsonObject {
    serde_json::from_value(value).expect("tool arguments are an object")
}

async fn mcp_call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    tool: &str,
    args: Value,
) -> Result<Value> {
    client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments(args)))
        .await?
        .structured_content
        .with_context(|| format!("MCP {tool} structured content"))
}

fn assert_equal_envelopes(rust: &impl Serialize, cli: &Value, mcp: &Value) -> Result<()> {
    let rust = serde_json::to_value(rust)?;
    ensure!(rust == *cli, "Rust/CLI mismatch\nRust: {rust}\nCLI: {cli}");
    ensure!(rust == *mcp, "Rust/MCP mismatch\nRust: {rust}\nMCP: {mcp}");
    Ok(())
}

fn package_relative(package_root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(package_root)?
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .context("UTF-8 package-relative path")
        })
        .collect::<Result<Vec<_>>>()?
        .join("/"))
}

#[tokio::test]
async fn rust_cli_and_in_memory_mcp_have_crm3_capability_parity() -> Result<()> {
    let root = repo_root();
    let examples = root.join("examples");
    let package = examples.join(PACKAGE);
    let capabilities = CAPABILITIES
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let rust_service = templiqx_local::compose(&examples)?;

    let (server_transport, client_transport) = tokio::io::duplex(256 * 1024);
    let mcp_service = templiqx_local::compose(&examples)?;
    let server_task = tokio::spawn(async move {
        let running = TempliqxMcp::new(mcp_service)
            .serve(server_transport)
            .await?;
        running.waiting().await?;
        anyhow::Ok(())
    });
    let client = ().serve(client_transport).await?;

    let rust_catalog = rust_service.catalog();
    let cli_catalog = cli_envelope(&examples, &["catalog"])?;
    let mcp_catalog = mcp_call(&client, "catalog", json!({})).await?;
    assert_equal_envelopes(&rust_catalog, &cli_catalog, &mcp_catalog)?;

    let rust_validate_package = rust_service.validate_package(PACKAGE);
    let cli_validate_package = cli_envelope(&examples, &["validate", PACKAGE])?;
    let mcp_validate_package =
        mcp_call(&client, "validate_package", json!({"package": PACKAGE})).await?;
    assert_equal_envelopes(
        &rust_validate_package,
        &cli_validate_package,
        &mcp_validate_package,
    )?;

    for contract in [EXTRACTION, DRAFTING] {
        let request_path = package.join(if contract == EXTRACTION {
            "evals/bli-61-request.json"
        } else {
            "evals/bli-62-request.json"
        });
        let output_path = package.join(if contract == EXTRACTION {
            "evals/bli-61-output.json"
        } else {
            "evals/bli-62-output.json"
        });
        let request_value = read_json(&request_path)?;
        let typed_request: RenderRequest = serde_json::from_value(request_value.clone())?;
        let fixture = read_json(&output_path)?;
        let request_path_str = request_path.to_str().context("UTF-8 request path")?;
        let output_path_str = output_path.to_str().context("UTF-8 output path")?;
        let interaction_args = json!({
            "package": PACKAGE,
            "contract": contract,
            "inputs": request_value["inputs"],
            "context": request_value["context"],
            "capabilities": capabilities,
        });

        let rust_validate = rust_service.validate_contract(PACKAGE, contract);
        let cli_validate = cli_envelope(&examples, &["validate", PACKAGE, contract])?;
        let mcp_validate = mcp_call(
            &client,
            "validate_contract",
            json!({"package": PACKAGE, "contract": contract}),
        )
        .await?;
        assert_equal_envelopes(&rust_validate, &cli_validate, &mcp_validate)?;

        let rust_compile =
            rust_service.compile_contract(PACKAGE, contract, &typed_request, &capabilities);
        let cli_compile = cli_envelope(
            &examples,
            &[
                "compile",
                PACKAGE,
                contract,
                "--values",
                request_path_str,
                "--capability",
                "structured_output",
            ],
        )?;
        let mcp_compile = mcp_call(&client, "compile_contract", interaction_args.clone()).await?;
        assert_equal_envelopes(&rust_compile, &cli_compile, &mcp_compile)?;

        let rust_render =
            rust_service.render_contract(PACKAGE, contract, &typed_request, &capabilities);
        let cli_render = cli_envelope(
            &examples,
            &[
                "render",
                PACKAGE,
                contract,
                "--values",
                request_path_str,
                "--capability",
                "structured_output",
            ],
        )?;
        let mcp_render = mcp_call(&client, "render_contract", interaction_args.clone()).await?;
        assert_equal_envelopes(&rust_render, &cli_render, &mcp_render)?;

        let rust_execute = rust_service.execute_contract(
            PACKAGE,
            contract,
            &typed_request,
            &capabilities,
            Some(fixture.clone()),
            false,
        );
        let cli_execute = cli_envelope(
            &examples,
            &[
                "execute",
                PACKAGE,
                contract,
                "--values",
                request_path_str,
                "--fixture-output",
                output_path_str,
                "--capability",
                "structured_output",
            ],
        )?;
        let mut execute_args = interaction_args;
        execute_args["fixture_output"] = fixture;
        let mcp_execute = mcp_call(&client, "execute_contract", execute_args).await?;
        assert_equal_envelopes(&rust_execute, &cli_execute, &mcp_execute)?;

        let rust_explain = rust_service.explain_contract(PACKAGE, contract);
        let cli_explain = cli_envelope(&examples, &["explain", PACKAGE, contract])?;
        let mcp_explain = mcp_call(
            &client,
            "explain_contract",
            json!({"package": PACKAGE, "contract": contract}),
        )
        .await?;
        assert_equal_envelopes(&rust_explain, &cli_explain, &mcp_explain)?;
    }

    let rust_test = rust_service.test_package(PACKAGE, &capabilities);
    let cli_test = cli_envelope(
        &examples,
        &["test", PACKAGE, "--capability", "structured_output"],
    )?;
    let mcp_test = mcp_call(
        &client,
        "test_package",
        json!({"package": PACKAGE, "capabilities": capabilities}),
    )
    .await?;
    assert_equal_envelopes(&rust_test, &cli_test, &mcp_test)?;

    for (left, right) in [(EXTRACTION, EXTRACTION), (EXTRACTION, DRAFTING)] {
        let rust_diff = rust_service.diff_contract(PACKAGE, left, PACKAGE, right);
        let cli_diff = cli_envelope(&examples, &["diff", PACKAGE, left, PACKAGE, right])?;
        let mcp_diff = mcp_call(
            &client,
            "diff_contract",
            json!({
                "left_package": PACKAGE,
                "left_contract": left,
                "right_package": PACKAGE,
                "right_contract": right,
            }),
        )
        .await?;
        assert_equal_envelopes(&rust_diff, &cli_diff, &mcp_diff)?;
    }

    let temp = tempfile::Builder::new()
        .prefix(".templiqx-parity-")
        .tempdir_in(&package)?;
    let aliases_path = package.join("migrations/v5-aliases.json");
    let aliases = read_json(&aliases_path)?;
    let source_fixture = package.join("templates/v5-contract-template.docx");
    let shared_source = temp.path().join("shared.docx");
    fs::copy(&source_fixture, &shared_source)?;

    let shared_source_relative = package_relative(&package, &shared_source)?;
    let rust_migrate = rust_service.migrate_legacy(&MigrateLegacyRequest {
        package: PACKAGE.into(),
        dialect: "v5".into(),
        source: shared_source_relative.clone(),
        aliases: aliases.clone(),
    });
    let cli_migrate = cli_envelope(
        &examples,
        &[
            "migrate",
            PACKAGE,
            "v5",
            &shared_source_relative,
            "--aliases",
            aliases_path.to_str().context("aliases")?,
        ],
    )?;
    let mcp_migrate = mcp_call(
        &client,
        "migrate_legacy",
        json!({"package": PACKAGE, "dialect": "v5", "source": shared_source_relative, "aliases": aliases}),
    )
    .await?;
    assert_equal_envelopes(&rust_migrate, &cli_migrate, &mcp_migrate)?;

    let rust_migration = rust_migrate.result.context("Rust migration result")?;
    let rust_template_relative = rust_migration
        .canonical_template
        .context("Rust canonical template")?;
    ensure!(cli_migrate["result"]["canonical_template"] == rust_template_relative);
    ensure!(mcp_migrate["result"]["canonical_template"] == rust_template_relative);
    let merge_data = read_json(package.join("evals/bli-62-output.json"))?["merge_data"].clone();
    let data_path = temp.path().join("merge-data.json");
    fs::write(&data_path, serde_json::to_vec_pretty(&merge_data)?)?;
    let shared_workspace = temp.path().join("workspace");
    let shared_artifact = shared_workspace.join(PACKAGE).join("shared-rendered.docx");
    let shared_artifact_relative = "shared-rendered.docx";
    let rust_render = rust_service.render_document(&RenderDocumentRequest {
        package: PACKAGE.into(),
        template: rust_template_relative.clone(),
        data: merge_data.clone(),
        output: shared_artifact_relative.into(),
        workspace: Some(shared_workspace.to_string_lossy().into_owned()),
    });
    let rust_artifact_fingerprint = file_fingerprint(&shared_artifact)?;
    let cli_render = cli_envelope(
        &examples,
        &[
            "render-document",
            PACKAGE,
            &rust_template_relative,
            data_path.to_str().context("merge data")?,
            shared_artifact_relative,
            "--workspace",
            shared_workspace.to_str().context("workspace")?,
        ],
    )?;
    let cli_artifact_fingerprint = file_fingerprint(&shared_artifact)?;
    let mcp_render = mcp_call(
        &client,
        "render_document",
        json!({"package": PACKAGE, "template": rust_template_relative, "data": merge_data, "output": shared_artifact_relative, "workspace": shared_workspace}),
    )
    .await?;
    let mcp_artifact_fingerprint = file_fingerprint(&shared_artifact)?;
    assert_equal_envelopes(&rust_render, &cli_render, &mcp_render)?;
    ensure!(rust_artifact_fingerprint == cli_artifact_fingerprint);
    ensure!(rust_artifact_fingerprint == mcp_artifact_fingerprint);

    client.cancel().await?;
    server_task.await??;
    Ok(())
}

fn assert_path_rejected<T>(envelope: &OperationEnvelope<T>) -> Result<()> {
    ensure!(!envelope.ok, "unsafe document path was accepted");
    ensure!(
        envelope
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TQX_PATH_INVALID"),
        "expected TQX_PATH_INVALID, got {:?}",
        envelope.diagnostics
    );
    Ok(())
}

#[test]
fn application_document_boundary_rejects_unconfined_paths_before_adapter_use() -> Result<()> {
    let root = repo_root();
    let examples = root.join("examples");
    let package = examples.join(PACKAGE);
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(&examples, workspace.path())?;
    let temp = tempfile::Builder::new()
        .prefix(".templiqx-security-")
        .tempdir_in(&package)?;
    let fixture = package.join("templates/v5-contract-template.docx");
    let source = temp.path().join("source.docx");
    fs::copy(&fixture, &source)?;
    let aliases = read_json(package.join("migrations/v5-aliases.json"))?;
    let canonical = source.with_file_name("source.templiqx-v5.docx");
    ensure!(!canonical.exists());

    let traversal_source = format!("../{PACKAGE}/{}", package_relative(&package, &source)?);
    for unsafe_source in [
        source.to_string_lossy().into_owned(),
        traversal_source,
        r"templates\v5-contract-template.docx".into(),
    ] {
        let envelope = service.migrate_legacy(&MigrateLegacyRequest {
            package: PACKAGE.into(),
            dialect: "v5".into(),
            source: unsafe_source,
            aliases: aliases.clone(),
        });
        assert_path_rejected(&envelope)?;
        ensure!(
            !canonical.exists(),
            "rejected migration created a canonical artifact"
        );
    }

    let data = read_json(package.join("evals/bli-62-output.json"))?["merge_data"].clone();
    let safe_template = "templates/v5-contract-template.docx";
    let outside_output = root.join("must-not-render.docx");
    let unsafe_renders = [
        RenderDocumentRequest {
            package: PACKAGE.into(),
            template: fixture.to_string_lossy().into_owned(),
            data: data.clone(),
            output: package_relative(&package, &temp.path().join("absolute-input.docx"))?,
            workspace: None,
        },
        RenderDocumentRequest {
            package: PACKAGE.into(),
            template: "../crm3/templates/v5-contract-template.docx".into(),
            data: data.clone(),
            output: package_relative(&package, &temp.path().join("traversal-input.docx"))?,
            workspace: None,
        },
        RenderDocumentRequest {
            package: PACKAGE.into(),
            template: r"templates\v5-contract-template.docx".into(),
            data: data.clone(),
            output: package_relative(&package, &temp.path().join("backslash-input.docx"))?,
            workspace: None,
        },
        RenderDocumentRequest {
            package: PACKAGE.into(),
            template: safe_template.into(),
            data: data.clone(),
            output: outside_output.to_string_lossy().into_owned(),
            workspace: None,
        },
        RenderDocumentRequest {
            package: PACKAGE.into(),
            template: safe_template.into(),
            data: data.clone(),
            output: "../outside.docx".into(),
            workspace: None,
        },
        RenderDocumentRequest {
            package: PACKAGE.into(),
            template: safe_template.into(),
            data: data.clone(),
            output: r"artifacts\outside.docx".into(),
            workspace: None,
        },
    ];
    for request in unsafe_renders {
        assert_path_rejected(&service.render_document(&request))?;
    }
    ensure!(!outside_output.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let input_link = temp.path().join("input-link.docx");
        symlink(&fixture, &input_link)?;
        let input_link_output = temp.path().join("input-link.templiqx-v5.docx");
        assert_path_rejected(&service.migrate_legacy(&MigrateLegacyRequest {
            package: PACKAGE.into(),
            dialect: "v5".into(),
            source: package_relative(&package, &input_link)?,
            aliases: aliases.clone(),
        }))?;
        ensure!(!input_link_output.exists());
        assert_path_rejected(&service.render_document(&RenderDocumentRequest {
            package: PACKAGE.into(),
            template: package_relative(&package, &input_link)?,
            data: data.clone(),
            output: "symlink-input.docx".into(),
            workspace: None,
        }))?;

        let outside = tempfile::tempdir()?;
        let outside_file = outside.path().join("outside.docx");
        fs::write(&outside_file, b"unchanged")?;
        let workspace_package = workspace.path().join(PACKAGE);
        fs::create_dir_all(&workspace_package)?;
        let output_link = workspace_package.join("output-link.docx");
        symlink(&outside_file, &output_link)?;
        assert_path_rejected(&service.render_document(&RenderDocumentRequest {
            package: PACKAGE.into(),
            template: safe_template.into(),
            data: data.clone(),
            output: "output-link.docx".into(),
            workspace: None,
        }))?;
        ensure!(fs::read(&outside_file)? == b"unchanged");

        let parent_link = workspace_package.join("parent-link");
        symlink(outside.path(), &parent_link)?;
        assert_path_rejected(&service.render_document(&RenderDocumentRequest {
            package: PACKAGE.into(),
            template: safe_template.into(),
            data,
            output: "parent-link/escaped.docx".into(),
            workspace: None,
        }))?;
        ensure!(!outside.path().join("escaped.docx").exists());
    }

    Ok(())
}
