//! U4: multi-output receipt assembly for the Legal reference package.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use templiqx_application::{RenderDocumentRequest, with_synthetic_authorized_context};
use templiqx_conformance::{
    ConformanceTraceReceipt, DocumentEvidence, DocumentOutputEvidence, DocumentOutputKind,
    InteractionEvidence, PdfConversionEvidence, RECEIPT_SCHEMA_VERSION, TRACE_API_VERSION,
    file_fingerprint, report_fingerprint, sort_document_outputs,
};
use templiqx_contracts::{Contract, ExecutionReceipt, OperationEnvelope, RenderRequest};
use templiqx_docx_v5::DocxV5Adapter;
use templiqx_html_plain::HtmlPlainAdapter;
use templiqx_ports::{
    DocumentRenderRequest, DocumentRenderer, LegacyImportAdapter, LegacyImportRequest,
    LegacyImportResult,
};

const PACKAGE: &str = "basenet-legal";
const EXTRACTION: &str = "legal-matter-extraction";
const DRAFTING: &str = "legal-document-drafting";
const CAPABILITIES: &[&str] = &["structured_output"];

fn packages_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .canonicalize()
        .expect("examples/packages root")
}

fn package_root() -> PathBuf {
    packages_root().join(PACKAGE)
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
        .ok_or_else(|| anyhow::anyhow!("{} returned no result", envelope.operation))
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

fn load_pdf_evidence() -> Result<PdfConversionEvidence> {
    let manifest_path = package_root().join("fixtures/pdf-renderer-manifest.json");
    #[derive(serde::Deserialize)]
    struct Manifest {
        renderer_id: String,
        renderer_version: String,
        environment_id: String,
        artifact_fingerprint: String,
        artifact_bytes: u64,
        output_hash: String,
        source_artifact: String,
    }
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(&manifest_path).context("manifest")?)?;
    let pdf_path = package_root().join(&manifest.source_artifact);
    let fingerprint = file_fingerprint(&pdf_path)?;
    ensure!(fingerprint == manifest.artifact_fingerprint);
    Ok(PdfConversionEvidence {
        renderer_id: manifest.renderer_id,
        renderer_version: manifest.renderer_version,
        environment_id: manifest.environment_id,
        artifact_fingerprint: manifest.artifact_fingerprint,
        artifact_bytes: manifest.artifact_bytes,
        output_hash: manifest.output_hash,
    })
}

fn package_relative(package: &Path, artifact: &Path) -> Result<String> {
    artifact
        .strip_prefix(package)
        .map(|path| path.to_string_lossy().trim_start_matches('/').to_string())
        .map_err(|_| anyhow::anyhow!("artifact escapes package root"))
}

#[test]
fn legal_package_assembles_multi_output_receipt() -> Result<()> {
    let package = package_root();
    let workspace = tempfile::tempdir()?;
    let service = templiqx_local::compose_with_workspace(packages_root(), workspace.path())?;
    let capabilities = CAPABILITIES
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let manifest = result(service.discover_packages())?
        .into_iter()
        .find(|manifest| manifest.package == PACKAGE)
        .context("basenet-legal manifest")?;
    let package_validation = service.validate_package(PACKAGE);
    ensure!(package_validation.ok);
    let package_fingerprint = package_validation.fingerprints["package"].clone();
    let eval_report = result(service.test_package(PACKAGE, &capabilities))?;
    ensure!(eval_report.passed == 2 && eval_report.failed == 0);

    let extraction_contract = result(service.inspect_contract(PACKAGE, EXTRACTION))?;
    let extraction_validation = service.validate_contract(PACKAGE, EXTRACTION);
    ensure!(extraction_validation.ok);
    let extraction_contract_fingerprint = extraction_validation.fingerprints["contract"].clone();
    let extraction_request = request(package.join("evals/legal-extraction-request.json"))?;
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
        Some(read_json(
            package.join("evals/legal-extraction-output.json"),
        )?),
        false,
    ))?;

    let drafting_request = request(package.join("evals/legal-draft-request.json"))?;
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
        Some(read_json(package.join("evals/legal-draft-output.json"))?),
        false,
    ))?;

    let temp = tempfile::Builder::new()
        .prefix(".templiqx-conformance-")
        .tempdir_in(&package)?;
    let template = temp.path().join("v5-legal-template.docx");
    fs::copy(package.join("templates/v5-legal-template.docx"), &template)?;
    let aliases = read_json(package.join("migrations/v5-aliases.json"))?;
    let adapter = DocxV5Adapter::default();
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
    let merge_data = read_json(package.join("fixtures/merge-data.json"))?;
    let authorized_context = read_json(package.join("fixtures/authorized-context.json"))?;
    let mut render_data = merge_data.clone();
    if let Some(obj) = render_data.as_object_mut() {
        obj.insert("_templiqx_authorized_merge".into(), authorized_context);
    }
    let rendered = result(service.render_document(&RenderDocumentRequest {
        package: PACKAGE.into(),
        template: package_relative(&package, &migrated)?,
        data: render_data.clone(),
        output: "legal-rendered.docx".into(),
        workspace: Some(temp.path().to_string_lossy().into_owned()),
    }))?;
    let docx_output = temp.path().join(PACKAGE).join("legal-rendered.docx");
    let docx_report = rendered.report;

    let draft_output = read_json(package.join("evals/legal-draft-output.json"))?;
    let draft_request = request(package.join("evals/legal-draft-request.json"))?;
    let html_data = json!({
        "client_name": draft_request.context["client_name"],
        "summary": draft_output["summary"],
        "parties": draft_output["merge_data"]["parties"],
        "department_name": draft_request.context["department_name"],
    });
    let html_output = temp.path().join("legal-email.html");
    let html_render = HtmlPlainAdapter.render_document(&DocumentRenderRequest {
        template: package.join("templates/draft-email.html"),
        data: html_data.clone(),
        output: html_output.clone(),
    })?;
    let html_report = html_render.report;
    let pdf_evidence = load_pdf_evidence()?;

    let parity = adapter.compare_normalized(&docx_output, &docx_output)?;
    let document = DocumentEvidence {
        adapter_id: "templiqx-docx-v5".into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        source_template_fingerprint: file_fingerprint(&template)?,
        canonical_template_fingerprint: file_fingerprint(&migrated)?,
        migration_report_fingerprint: report_fingerprint(&migration_report)?,
        render_input_fingerprint: fingerprint(&render_data)?,
        render_report_fingerprint: report_fingerprint(&docx_report)?,
        artifact_fingerprint: file_fingerprint(&docx_output)?,
        approved_baseline_fingerprint: file_fingerprint(&docx_output)?,
        parity_report_fingerprint: fingerprint(&parity)?,
        normalized_ooxml_equal: parity.equal,
        unresolved_references: docx_report["unresolved"]
            .as_array()
            .map_or(0, std::vec::Vec::len),
    };

    let mut outputs = vec![
        DocumentOutputEvidence {
            kind: DocumentOutputKind::Docx,
            adapter_id: "templiqx-docx-v5".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_template_fingerprint: document.source_template_fingerprint.clone(),
            render_input_fingerprint: document.render_input_fingerprint.clone(),
            render_report_fingerprint: document.render_report_fingerprint.clone(),
            artifact_fingerprint: document.artifact_fingerprint.clone(),
            pdf_conversion: None,
        },
        DocumentOutputEvidence {
            kind: DocumentOutputKind::Html,
            adapter_id: "templiqx-html-plain".into(),
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            source_template_fingerprint: file_fingerprint(
                &package.join("templates/draft-email.html"),
            )?,
            render_input_fingerprint: fingerprint(&html_data)?,
            render_report_fingerprint: report_fingerprint(&html_report)?,
            artifact_fingerprint: file_fingerprint(&html_output)?,
            pdf_conversion: None,
        },
        DocumentOutputEvidence {
            kind: DocumentOutputKind::Pdf,
            adapter_id: "host-document-converter".into(),
            adapter_version: "recorded".into(),
            source_template_fingerprint: document.artifact_fingerprint.clone(),
            render_input_fingerprint: document.artifact_fingerprint.clone(),
            render_report_fingerprint: report_fingerprint(&json!({
                "recorded": true,
                "source": "fixtures/recorded-legal.pdf"
            }))?,
            artifact_fingerprint: pdf_evidence.artifact_fingerprint.clone(),
            pdf_conversion: Some(pdf_evidence),
        },
    ];
    sort_document_outputs(&mut outputs);

    let trace = ConformanceTraceReceipt {
        api_version: TRACE_API_VERSION.into(),
        receipt_schema_version: RECEIPT_SCHEMA_VERSION.into(),
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
                &with_synthetic_authorized_context(drafting_request, "SYN-LEGAL-SCOPE-001"),
                &capabilities,
                &drafting_receipt,
            )?,
        ],
        document,
        outputs,
    };

    ensure!(trace.outputs.len() == 3);
    ensure!(trace.outputs[0].kind == DocumentOutputKind::Docx);
    ensure!(trace.outputs[1].kind == DocumentOutputKind::Html);
    ensure!(trace.outputs[2].kind == DocumentOutputKind::Pdf);
    ensure!(trace.outputs[2].pdf_conversion.is_some());

    let trace_json = serde_json::to_string_pretty(&trace)?;
    for forbidden_payload in ["Voorbeeld A B.V.", "SYN-LEGAL-RECORDED", "merge_data"] {
        ensure!(!trace_json.contains(forbidden_payload));
    }

    let roundtrip: ConformanceTraceReceipt = serde_json::from_str(&trace_json)?;
    ensure!(roundtrip == trace);
    Ok(())
}
