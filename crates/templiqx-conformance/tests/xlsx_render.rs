//! U9: deterministic native XLSX plus pinned CSV/XML serializers.

use std::{io::Cursor, path::PathBuf};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use templiqx_ports::{DocumentRenderRequest, DocumentRenderer};
use templiqx_tabular::{CSV_RENDERER_ID, CsvAdapter, XML_RENDERER_ID, XmlAdapter};
use templiqx_xlsx::{ENVIRONMENT_ID, RENDERER_ID, RENDERER_VERSION, XlsxAdapter};
use zip::ZipArchive;

const CSV_GOLDEN: &str = concat!(
    "Team,Matters,Note\n",
    "Employment,12,\"Priority, \"\"A\"\" <review>\"\n",
    "Corporate,8,Standard\n",
);

const XML_GOLDEN: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
    "<table>\n",
    "  <columns>\n",
    "    <column key=\"team\">Team</column>\n",
    "    <column key=\"matters\">Matters</column>\n",
    "    <column key=\"note\">Note</column>\n",
    "  </columns>\n",
    "  <rows>\n",
    "    <row>\n",
    "      <cell column=\"team\">Employment</cell>\n",
    "      <cell column=\"matters\">12</cell>\n",
    "      <cell column=\"note\">Priority, &quot;A&quot; &lt;review&gt;</cell>\n",
    "    </row>\n",
    "    <row>\n",
    "      <cell column=\"team\">Corporate</cell>\n",
    "      <cell column=\"matters\">8</cell>\n",
    "      <cell column=\"note\">Standard</cell>\n",
    "    </row>\n",
    "  </rows>\n",
    "</table>\n",
);

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages/basenet-legal/fixtures/tabular-report.json")
}

fn fixture() -> Result<Value> {
    serde_json::from_slice(&std::fs::read(fixture_path()).context("read tabular fixture")?)
        .context("parse tabular fixture")
}

fn merge_data() -> Result<Value> {
    fixture()?
        .get("merge_data")
        .cloned()
        .context("tabular fixture merge_data")
}

#[test]
fn tabular_definition_emits_byte_stable_xlsx_with_native_chart() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let first_output = workspace.path().join("first.xlsx");
    let second_output = workspace.path().join("second.xlsx");
    let first = XlsxAdapter.render_document(&DocumentRenderRequest {
        template: fixture_path(),
        data: merge_data()?,
        output: first_output,
    })?;
    let second = XlsxAdapter.render_document(&DocumentRenderRequest {
        template: fixture_path(),
        data: merge_data()?,
        output: second_output,
    })?;
    let first_bytes = std::fs::read(&first.artifact)?;
    let second_bytes = std::fs::read(&second.artifact)?;

    ensure!(first_bytes.starts_with(b"PK"), "XLSX must be a ZIP package");
    ensure!(
        first_bytes == second_bytes,
        "two XLSX renders must be byte-identical"
    );
    let mut archive = ZipArchive::new(Cursor::new(&first_bytes))?;
    archive
        .by_name("xl/charts/chart1.xml")
        .context("native Excel chart entry")?;

    ensure!(first.report["renderer_id"] == RENDERER_ID);
    ensure!(first.report["renderer_version"] == RENDERER_VERSION);
    ensure!(first.report["environment_id"] == ENVIRONMENT_ID);
    ensure!(first.report["artifact_fingerprint"] == second.report["artifact_fingerprint"]);
    ensure!(first.report["artifact_fingerprint"] == first.report["output_hash"]);
    ensure!(first.report["native_chart"] == "column");
    ensure!(first.report["status"] == "ok");
    Ok(())
}

#[test]
fn csv_and_xml_match_goldens_and_escape_merge_values() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let csv_output = workspace.path().join("report.csv");
    let xml_output = workspace.path().join("report.xml");
    let csv = CsvAdapter.render_document(&DocumentRenderRequest {
        template: fixture_path(),
        data: merge_data()?,
        output: csv_output.clone(),
    })?;
    let xml = XmlAdapter.render_document(&DocumentRenderRequest {
        template: fixture_path(),
        data: merge_data()?,
        output: xml_output.clone(),
    })?;

    let csv_bytes = std::fs::read(&csv_output)?;
    let xml_bytes = std::fs::read(&xml_output)?;
    ensure!(csv_bytes == CSV_GOLDEN.as_bytes());
    ensure!(xml_bytes == XML_GOLDEN.as_bytes());
    ensure!(csv.report["renderer_id"] == CSV_RENDERER_ID);
    ensure!(xml.report["renderer_id"] == XML_RENDERER_ID);
    ensure!(csv.report["artifact_fingerprint"] == csv.report["output_hash"]);
    ensure!(xml.report["artifact_fingerprint"] == xml.report["output_hash"]);

    let second_csv = workspace.path().join("report-second.csv");
    let second_xml = workspace.path().join("report-second.xml");
    CsvAdapter.render_document(&DocumentRenderRequest {
        template: fixture_path(),
        data: merge_data()?,
        output: second_csv.clone(),
    })?;
    XmlAdapter.render_document(&DocumentRenderRequest {
        template: fixture_path(),
        data: merge_data()?,
        output: second_xml.clone(),
    })?;
    ensure!(std::fs::read(second_csv)? == csv_bytes);
    ensure!(std::fs::read(second_xml)? == xml_bytes);
    Ok(())
}
