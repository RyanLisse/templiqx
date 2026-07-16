//! U8: deterministic Typst markup plus the recorded host-owned PDF seam.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use templiqx_conformance::PdfConversionEvidence;
use templiqx_ports::{DocumentRenderRequest, DocumentRenderer};
use templiqx_typst::{
    ENVIRONMENT_ID, RENDERER_ID, RENDERER_VERSION, TypstReportAdapter, format_date, format_number,
};

const PACKAGE: &str = "basenet-legal";
const PINNED_TYPST_SHA256: &str =
    "24a6493cd75837742ef0c7ec6f8525d094ad17600ef65340b3e2f37fb2768c75";

fn package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .join(PACKAGE)
        .canonicalize()
        .expect("basenet-legal package root")
}

fn bytes_fingerprint(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn fixture_data() -> Result<Value> {
    serde_json::from_slice(
        &std::fs::read(package_root().join("fixtures/merge-data.json"))
            .context("read frozen merge-data fixture")?,
    )
    .context("parse frozen merge-data fixture")
}

fn render_fixture(output: PathBuf) -> Result<templiqx_ports::DocumentRenderResult> {
    TypstReportAdapter
        .render_document(&DocumentRenderRequest {
            template: package_root().join("definitions/dunning-letter-v1.typ"),
            data: fixture_data()?,
            output,
        })
        .context("render Typst fixture")
}

#[test]
fn frozen_definition_and_fixture_emit_pinned_typst_markup() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let first = render_fixture(workspace.path().join("first.typ"))?;
    let second = render_fixture(workspace.path().join("second.typ"))?;
    let first_bytes = std::fs::read(&first.artifact)?;
    let second_bytes = std::fs::read(&second.artifact)?;

    ensure!(
        first_bytes == second_bytes,
        "Typst markup drifted across renders"
    );
    let fingerprint = bytes_fingerprint(&first_bytes);
    ensure!(
        fingerprint == PINNED_TYPST_SHA256,
        "update only after deliberate fixture review: actual={fingerprint}"
    );
    ensure!(first.report["renderer_id"] == RENDERER_ID);
    ensure!(first.report["renderer_version"] == RENDERER_VERSION);
    ensure!(first.report["environment_id"] == ENVIRONMENT_ID);
    ensure!(first.report["artifact_fingerprint"] == fingerprint);
    ensure!(first.report["output_hash"] == fingerprint);
    ensure!(first.report["status"] == "ok");
    Ok(())
}

#[test]
fn native_typst_chart_and_locale_values_are_byte_stable() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let first = render_fixture(workspace.path().join("chart-first.typ"))?;
    let second = render_fixture(workspace.path().join("chart-second.typ"))?;
    let first_bytes = std::fs::read(&first.artifact)?;
    let second_bytes = std::fs::read(&second.artifact)?;
    let markup = std::str::from_utf8(&first_bytes)?;

    ensure!(markup.contains("// templiqx-native-chart: claims"));
    ensure!(markup.contains("#table("));
    ensure!(markup.contains("45.750,00"));
    ensure!(markup.contains("42.550,00"));
    ensure!(format_number(&serde_json::json!(45_750), "nl-NL").as_deref() == Some("45.750,00"));
    ensure!(format_date("2026-07-31", "nl-NL").as_deref() == Some("31-07-2026"));
    ensure!(first_bytes == second_bytes, "native chart markup drifted");
    Ok(())
}

#[test]
fn missing_or_unauthorized_field_is_fail_closed() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let template = workspace.path().join("restricted.typ");
    let output = workspace.path().join("restricted-output.typ");
    std::fs::write(
        &template,
        "Allowed: {{ public.name }}\nRestricted: {{ private.secret }}\n",
    )?;
    let result = TypstReportAdapter.render_document(&DocumentRenderRequest {
        template,
        data: serde_json::json!({"public": {"name": "zichtbaar"}}),
        output: output.clone(),
    })?;
    let markup = std::fs::read_to_string(output)?;

    ensure!(result.report["status"] == "failed_closed");
    ensure!(result.report["unresolved_fields"] == serde_json::json!(["private.secret"]));
    ensure!(result.report["diagnostics"][0]["code"] == "typst.unresolved_field");
    ensure!(result.report["diagnostics"][0]["field"] == "private.secret");
    ensure!(markup.contains("Allowed: #text(\"zichtbaar\")"));
    ensure!(markup.contains("Restricted: \n"));
    ensure!(!markup.contains("secret"));
    ensure!(!markup.contains("guessed"));

    let chart_template = workspace.path().join("restricted-chart.typ");
    let chart_output = workspace.path().join("restricted-chart-output.typ");
    std::fs::write(&chart_template, "{{#chart rows locale=nl-NL}}\n")?;
    let chart_result = TypstReportAdapter.render_document(&DocumentRenderRequest {
        template: chart_template.clone(),
        data: serde_json::json!({
            "rows": [
                {"label": "toegestaan", "amount": 10},
                {"label": "bedrag niet geautoriseerd"}
            ]
        }),
        output: chart_output,
    })?;
    ensure!(chart_result.report["status"] == "failed_closed");
    ensure!(chart_result.report["unresolved_fields"] == serde_json::json!(["rows[1].amount"]));

    let empty_chart_result = TypstReportAdapter.render_document(&DocumentRenderRequest {
        template: chart_template,
        data: serde_json::json!({"rows": [{}]}),
        output: workspace.path().join("empty-chart-output.typ"),
    })?;
    ensure!(empty_chart_result.report["status"] == "failed_closed");
    ensure!(empty_chart_result.report["unresolved_fields"] == serde_json::json!(["rows"]));
    Ok(())
}

#[derive(Debug, Deserialize)]
struct TypstRendererManifest {
    source_artifact: String,
    renderer_id: String,
    renderer_version: String,
    environment_id: String,
    artifact_fingerprint: String,
    artifact_bytes: u64,
    output_hash: String,
}

fn load_recorded_typst_pdf_evidence() -> Result<(PdfConversionEvidence, Vec<u8>)> {
    let root = package_root();
    let manifest_path = root.join("fixtures/typst-renderer-manifest.json");
    let manifest: TypstRendererManifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).context("Typst manifest")?)?;
    let bytes =
        std::fs::read(root.join(&manifest.source_artifact)).context("recorded Typst PDF")?;
    let fingerprint = bytes_fingerprint(&bytes);
    ensure!(fingerprint == manifest.artifact_fingerprint);
    ensure!(fingerprint == manifest.output_hash);
    ensure!(u64::try_from(bytes.len())? == manifest.artifact_bytes);
    Ok((
        PdfConversionEvidence {
            renderer_id: manifest.renderer_id,
            renderer_version: manifest.renderer_version,
            environment_id: manifest.environment_id,
            artifact_fingerprint: manifest.artifact_fingerprint,
            artifact_bytes: manifest.artifact_bytes,
            output_hash: manifest.output_hash,
        },
        bytes,
    ))
}

#[test]
fn recorded_typst_pdf_matches_host_renderer_manifest() -> Result<()> {
    let (first, bytes) = load_recorded_typst_pdf_evidence()?;
    let (second, _) = load_recorded_typst_pdf_evidence()?;
    ensure!(
        first == second,
        "recorded Typst PDF evidence must be stable"
    );
    ensure!(first.renderer_id == RENDERER_ID);
    ensure!(first.renderer_version == RENDERER_VERSION);
    ensure!(first.environment_id == "recorded-typst-pdf-v1");
    ensure!(bytes.starts_with(b"%PDF-"));
    Ok(())
}
