//! Deterministic local benchmark harness for contract and document operations.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};
use templiqx_application::{InspectDocumentRequest, RenderDocumentRequest};
use templiqx_contracts::{RenderRequest, fingerprint_bytes};
use templiqx_docx_v5::{DocxV5Adapter, Limits};
use templiqx_ports::LegacyImportAdapter;

const SCHEMA_VERSION: &str = "templiqx-bench/v1";

#[derive(Debug, Serialize)]
struct BenchReport {
    schema_version: &'static str,
    generated_at_unix_ms: u128,
    cases: Vec<BenchCase>,
}

#[derive(Debug, Serialize)]
struct BenchCase {
    id: String,
    category: String,
    iterations: u32,
    duration_ms: Vec<u128>,
    median_duration_ms: u128,
    functional_fingerprint: String,
    output_bytes: Option<u64>,
    ok: bool,
}

fn main() -> Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(repo_root);
    let report = run(&root)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn run(root: &Path) -> Result<BenchReport> {
    let cases = vec![
        bench_contract_validate_compile(root)?,
        bench_document_inspect(root)?,
        bench_document_render(root)?,
        bench_hostile_archive_rejection(root)?,
    ];
    Ok(BenchReport {
        schema_version: SCHEMA_VERSION,
        generated_at_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        cases,
    })
}

fn bench_contract_validate_compile(root: &Path) -> Result<BenchCase> {
    let workspace = tempfile::tempdir()?;
    let service =
        templiqx_local::compose_with_workspace(root.join("examples/packages"), workspace.path())?;
    let request = RenderRequest {
        inputs: [("name".into(), serde_json::json!("Ryan"))].into(),
        context: [("organization".into(), serde_json::json!("Blinqx"))].into(),
    };
    let capabilities = ["structured_output".to_string()];
    let mut durations = Vec::new();
    let mut fingerprint = String::new();
    let mut ok = false;
    for _ in 0..3 {
        let started = Instant::now();
        let validated = service.validate_contract("demo", "greeting");
        let compiled = service.compile_contract("demo", "greeting", &request, &capabilities);
        durations.push(started.elapsed().as_millis());
        ok = validated.ok && compiled.ok;
        if let Some(interaction) = compiled.result {
            fingerprint = fingerprint_bytes(&serde_json::to_vec(&interaction)?);
        }
    }
    Ok(case(
        "contract-validate-compile-demo",
        "contract",
        durations,
        fingerprint,
        None,
        ok,
    ))
}

fn bench_document_inspect(root: &Path) -> Result<BenchCase> {
    let fixture = root.join("examples/legacy-corpus/fixtures/v5-nested-table");
    let package_root = tempfile::tempdir()?;
    fs::create_dir_all(package_root.path().join("demo"))?;
    fs::copy(
        fixture.join("source.docx"),
        package_root.path().join("demo/source.docx"),
    )?;
    let service =
        templiqx_local::compose_with_workspace(package_root.path(), tempfile::tempdir()?.path())?;
    let aliases: Value = serde_json::from_slice(&fs::read(fixture.join("aliases.json"))?)?;
    let mut durations = Vec::new();
    let mut fingerprint = String::new();
    let mut ok = false;
    for _ in 0..3 {
        let started = Instant::now();
        let envelope = service.inspect_document(&InspectDocumentRequest {
            package: "demo".into(),
            dialect: "v5".into(),
            template: "source.docx".into(),
            aliases: aliases.clone(),
        });
        durations.push(started.elapsed().as_millis());
        ok = envelope.ok;
        if let Some(result) = envelope.result {
            fingerprint = fingerprint_bytes(&serde_json::to_vec(&result.report)?);
        }
    }
    Ok(case(
        "document-inspect-v5-nested-table",
        "document",
        durations,
        fingerprint,
        None,
        ok,
    ))
}

fn bench_document_render(root: &Path) -> Result<BenchCase> {
    let fixture = root.join("examples/legacy-corpus/fixtures/v5-nested-table");
    let package_root = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    fs::create_dir_all(package_root.path().join("demo"))?;
    let source = package_root.path().join("demo/source.docx");
    fs::copy(fixture.join("source.docx"), &source)?;
    let adapter = DocxV5Adapter::default();
    let migrated = adapter
        .migrate(&templiqx_ports::LegacyImportRequest {
            dialect: "v5".into(),
            source,
            aliases: serde_json::from_slice(&fs::read(fixture.join("aliases.json"))?)?,
        })
        .context("migrate fixture")?
        .canonical_template
        .context("canonical template")?;
    let template_relative = "source.templiqx-v5.docx";
    fs::rename(
        migrated,
        package_root
            .path()
            .join(format!("demo/{template_relative}")),
    )?;
    let service = templiqx_local::compose_with_workspace(package_root.path(), workspace.path())?;
    let data: Value = serde_json::from_slice(&fs::read(fixture.join("render-data.json"))?)?;
    let mut durations = Vec::new();
    let mut fingerprint = String::new();
    let mut output_bytes = None;
    let mut ok = false;
    for _ in 0..3 {
        let output = "rendered.docx";
        let started = Instant::now();
        let envelope = service.render_document(&RenderDocumentRequest {
            package: "demo".into(),
            template: template_relative.into(),
            data: data.clone(),
            output: output.into(),
            workspace: None,
        });
        durations.push(started.elapsed().as_millis());
        ok = envelope.ok;
        let artifact = workspace.path().join("demo/rendered.docx");
        if artifact.exists() {
            let bytes = fs::read(&artifact)?;
            fingerprint = fingerprint_bytes(&bytes);
            output_bytes = Some(bytes.len() as u64);
        }
    }
    Ok(case(
        "document-render-v5-nested-table",
        "document",
        durations,
        fingerprint,
        output_bytes,
        ok,
    ))
}

fn bench_hostile_archive_rejection(root: &Path) -> Result<BenchCase> {
    let fixture = root.join("examples/legacy-corpus/fixtures/invalid-oversized-entry/source.docx");
    let limits = Limits {
        max_entries: 512,
        max_entry_bytes: 1024,
        max_total_bytes: 64 * 1024 * 1024,
    };
    let mut durations = Vec::new();
    let mut ok = false;
    for _ in 0..3 {
        let started = Instant::now();
        let error = DocxV5Adapter::new(limits)
            .analyze(&fixture, &serde_json::json!({}))
            .unwrap_err();
        durations.push(started.elapsed().as_millis());
        ok = error.to_string().contains("limit") || error.to_string().contains("unsafe");
    }
    Ok(case(
        "hostile-archive-rejection",
        "security",
        durations,
        "rejected".into(),
        None,
        ok,
    ))
}

fn case(
    id: &str,
    category: &str,
    durations: Vec<u128>,
    functional_fingerprint: String,
    output_bytes: Option<u64>,
    ok: bool,
) -> BenchCase {
    let mut sorted = durations.clone();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    BenchCase {
        id: id.into(),
        category: category.into(),
        iterations: durations.len() as u32,
        duration_ms: durations,
        median_duration_ms: median,
        functional_fingerprint,
        output_bytes,
        ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::ensure;

    #[test]
    fn fixture_paths_exist() {
        let root = repo_root();
        for path in [
            "examples/packages/demo/templiqx.yaml",
            "examples/legacy-corpus/fixtures/v5-nested-table/source.docx",
            "examples/legacy-corpus/fixtures/invalid-oversized-entry/source.docx",
        ] {
            assert!(root.join(path).exists(), "missing fixture {path}");
        }
    }

    #[test]
    fn report_schema_is_stable_and_cases_succeed() -> Result<()> {
        let report = run(&repo_root())?;
        ensure!(report.schema_version == SCHEMA_VERSION);
        ensure!(!report.cases.is_empty());
        for case in &report.cases {
            ensure!(case.iterations >= 1);
            ensure!(!case.id.is_empty());
            ensure!(case.ok, "benchmark case failed: {}", case.id);
        }
        Ok(())
    }
}
