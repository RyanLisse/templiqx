//! U3: deterministic recorded source-to-PDF fixture proves the host-owned
//! conversion seam without wiring a repository converter into default composition.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use templiqx_conformance::PdfConversionEvidence;

const PACKAGE: &str = "basenet-legal";

fn package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .join(PACKAGE)
        .canonicalize()
        .expect("basenet-legal package root")
}

#[derive(Debug, Deserialize)]
struct PdfRendererManifest {
    source_artifact: String,
    renderer_id: String,
    renderer_version: String,
    environment_id: String,
    artifact_fingerprint: String,
    artifact_bytes: u64,
    output_hash: String,
}

fn bytes_fingerprint(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn load_recorded_pdf_evidence() -> Result<(PdfConversionEvidence, Vec<u8>)> {
    let root = package_root();
    let manifest_path = root.join("fixtures/pdf-renderer-manifest.json");
    let manifest: PdfRendererManifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).context("manifest")?)?;
    let pdf_path = root.join(&manifest.source_artifact);
    let bytes = std::fs::read(&pdf_path).context("recorded pdf")?;
    let fingerprint = bytes_fingerprint(&bytes);
    ensure!(
        fingerprint == manifest.artifact_fingerprint,
        "artifact fingerprint drift: manifest={}, actual={}",
        manifest.artifact_fingerprint,
        fingerprint
    );
    ensure!(
        fingerprint == manifest.output_hash,
        "output hash must match recorded artifact fingerprint"
    );
    ensure!(
        u64::try_from(bytes.len()).expect("pdf size") == manifest.artifact_bytes,
        "artifact byte size drift"
    );
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
fn recorded_legal_pdf_fixture_matches_renderer_manifest() -> Result<()> {
    let (evidence, bytes) = load_recorded_pdf_evidence()?;
    ensure!(evidence.renderer_id == "basenet-recorded-pdf");
    ensure!(evidence.environment_id == "conformance-recorded-v1");
    ensure!(bytes.starts_with(b"%PDF-"));
    Ok(())
}

#[test]
fn recorded_pdf_evidence_is_deterministic_across_reads() -> Result<()> {
    let (first, _) = load_recorded_pdf_evidence()?;
    let (second, _) = load_recorded_pdf_evidence()?;
    ensure!(first == second, "recorded PDF evidence must be stable");
    Ok(())
}

#[test]
fn recorded_pdf_evidence_serializes_without_document_bytes() -> Result<()> {
    let (evidence, bytes) = load_recorded_pdf_evidence()?;
    let json = serde_json::to_string(&evidence)?;
    ensure!(!json.contains("SYN-LEGAL-RECORDED"));
    ensure!(
        !json
            .as_bytes()
            .windows(bytes.len())
            .any(|window| window == bytes)
    );
    Ok(())
}
