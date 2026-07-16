//! Determinism bench for frozen basenet-legal DOCX renders (BLI-230 R4).

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use templiqx_contracts::fingerprint_bytes;
use templiqx_docx_v5::DocxV5Adapter;
use templiqx_ports::{
    DocumentRenderRequest, DocumentRenderer, LegacyImportAdapter, LegacyImportRequest,
};

const DEFAULT_ITERATIONS: u32 = 100;

#[derive(Debug, Serialize)]
pub struct DeterminismReceipt {
    pub schema_version: &'static str,
    pub iterations: u32,
    pub distinct_hash_count: usize,
    pub distinct_hash: String,
    pub sensitivity_hash: String,
    pub duration_ms: Vec<u128>,
    pub total_wall_ms: u128,
    pub ok: bool,
}

/// Migrate the frozen V5 legal template once, then render `iterations` times.
pub fn run_report_determinism(root: &Path, iterations: u32) -> Result<DeterminismReceipt> {
    ensure!(iterations >= 1, "iterations must be >= 1");
    let package = root.join("examples/packages/basenet-legal");
    let workspace = tempfile::tempdir().context("determinism tempdir")?;
    let migrated = migrate_legal_template(&package, workspace.path())?;
    let merge_data = read_json(package.join("fixtures/merge-data.json"))?;

    let mut hashes = BTreeSet::new();
    let mut durations = Vec::with_capacity(iterations as usize);
    let wall_started = Instant::now();
    for i in 0..iterations {
        let output = workspace.path().join(format!("render-{i:04}.docx"));
        let started = Instant::now();
        DocxV5Adapter::default()
            .render_document(&DocumentRenderRequest {
                template: migrated.clone(),
                data: merge_data.clone(),
                output: output.clone(),
            })
            .context("determinism render")?;
        durations.push(started.elapsed().as_millis());
        let bytes = fs::read(&output).context("read determinism artifact")?;
        hashes.insert(fingerprint_bytes(&bytes));
    }
    let total_wall_ms = wall_started.elapsed().as_millis();

    ensure!(
        hashes.len() == 1,
        "determinism violated: {} distinct hashes over {iterations} renders",
        hashes.len()
    );
    let distinct_hash = hashes.iter().next().expect("one hash").clone();

    let mut mutated = merge_data.clone();
    if let Some(client) = mutated.get_mut("client").and_then(Value::as_object_mut) {
        client.insert(
            "name".into(),
            Value::String("Mutated Determinism Client".into()),
        );
    } else {
        anyhow::bail!("merge-data.client.name missing for sensitivity check");
    }
    let sensitivity_output = workspace.path().join("sensitivity.docx");
    DocxV5Adapter::default()
        .render_document(&DocumentRenderRequest {
            template: migrated,
            data: mutated,
            output: sensitivity_output.clone(),
        })
        .context("sensitivity render")?;
    let sensitivity_hash = fingerprint_bytes(&fs::read(sensitivity_output)?);
    ensure!(
        sensitivity_hash != distinct_hash,
        "sensitivity check failed: mutated merge produced identical hash"
    );

    Ok(DeterminismReceipt {
        schema_version: "templiqx-bench/report-determinism/v1",
        iterations,
        distinct_hash_count: 1,
        distinct_hash,
        sensitivity_hash,
        duration_ms: durations,
        total_wall_ms,
        ok: true,
    })
}

pub fn run_report_determinism_default(root: &Path) -> Result<DeterminismReceipt> {
    run_report_determinism(root, DEFAULT_ITERATIONS)
}

pub(crate) fn migrate_legal_template(package: &Path, workspace: &Path) -> Result<PathBuf> {
    let source = workspace.join("v5-legal-template.docx");
    fs::copy(package.join("templates/v5-legal-template.docx"), &source)
        .context("copy legal template")?;
    let aliases = read_json(package.join("migrations/v5-aliases.json"))?;
    let migrated = DocxV5Adapter::default()
        .migrate(&LegacyImportRequest {
            dialect: "v5".into(),
            source,
            aliases,
        })
        .context("migrate legal template")?
        .canonical_template
        .context("migrated canonical template")?;
    Ok(migrated)
}

pub(crate) fn read_json(path: PathBuf) -> Result<Value> {
    serde_json::from_slice(&fs::read(&path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}
