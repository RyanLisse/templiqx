//! Fan-out bench: 1,000-record mailing render without a model (BLI-230 R5).

use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use templiqx_docx_v5::DocxV5Adapter;
use templiqx_ports::{DocumentRenderRequest, DocumentRenderer};
use zip::ZipArchive;

use crate::report_determinism::migrate_legal_template;

#[derive(Debug, Serialize)]
pub struct FanoutReceipt {
    pub schema_version: &'static str,
    pub record_count: usize,
    pub corrupt_count: usize,
    pub validity_rate: f64,
    pub wall_clock_ms: u128,
    pub throughput_records_per_sec: f64,
    pub ok: bool,
}

/// Render each fan-out record through the migrated legal DOCX template.
pub fn run_report_fanout(root: &Path) -> Result<FanoutReceipt> {
    let package = root.join("examples/packages/basenet-legal");
    let workspace = tempfile::tempdir().context("fanout tempdir")?;
    let migrated = migrate_legal_template(&package, workspace.path())?;
    let records: Vec<Value> =
        serde_json::from_slice(&fs::read(package.join("fixtures/fanout-records.json"))?)
            .context("parse fanout-records.json")?;
    ensure!(
        records.len() == 1000,
        "fanout fixture must contain exactly 1000 records, found {}",
        records.len()
    );

    let mut corrupt_count = 0usize;
    let started = Instant::now();
    for (index, record) in records.iter().enumerate() {
        let output = workspace.path().join(format!("fanout-{index:04}.docx"));
        DocxV5Adapter::default()
            .render_document(&DocumentRenderRequest {
                template: migrated.clone(),
                data: record.clone(),
                output: output.clone(),
            })
            .with_context(|| format!("fanout render {index}"))?;
        if !docx_well_formed(&output)? {
            corrupt_count += 1;
        }
    }
    let wall_clock_ms = started.elapsed().as_millis();
    let record_count = records.len();
    let validity_rate = if record_count == 0 {
        0.0
    } else {
        (record_count - corrupt_count) as f64 / record_count as f64
    };
    let throughput_records_per_sec = if wall_clock_ms == 0 {
        f64::INFINITY
    } else {
        (record_count as f64) / (wall_clock_ms as f64 / 1000.0)
    };
    ensure!(
        corrupt_count == 0,
        "fanout produced {corrupt_count} corrupt DOCX"
    );

    Ok(FanoutReceipt {
        schema_version: "templiqx-bench/report-fanout/v1",
        record_count,
        corrupt_count,
        validity_rate,
        wall_clock_ms,
        throughput_records_per_sec,
        ok: true,
    })
}

fn docx_well_formed(path: &Path) -> Result<bool> {
    let bytes = fs::read(path).context("read fanout artifact")?;
    if !bytes.starts_with(b"PK") {
        return Ok(false);
    }
    let mut archive = match ZipArchive::new(Cursor::new(bytes)) {
        Ok(archive) => archive,
        Err(_) => return Ok(false),
    };
    // basenet-legal fixtures are minimal OOXML packages (document/header/footer).
    // Prefer Content_Types when present; otherwise require a readable document part.
    if archive.by_name("[Content_Types].xml").is_ok() {
        return Ok(true);
    }
    let mut entry = match archive.by_name("word/document.xml") {
        Ok(entry) => entry,
        Err(_) => return Ok(false),
    };
    let mut xml = String::new();
    entry.read_to_string(&mut xml)?;
    Ok(xml.contains("<w:document") || xml.contains("w:document"))
}
