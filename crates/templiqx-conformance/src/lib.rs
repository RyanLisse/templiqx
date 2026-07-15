//! CRM3 conformance evidence types for the Templiqx POC.
//!
//! This crate is deliberately host-neutral. It records only content
//! fingerprints and bounded reports; model prompts, source text, model output,
//! and document bytes never enter the trace receipt.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub const TRACE_API_VERSION: &str = "templiqx.conformance/v1alpha1";
pub const RECEIPT_SCHEMA_VERSION: &str = "1";

fn default_receipt_schema_version() -> String {
    RECEIPT_SCHEMA_VERSION.into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InteractionEvidence {
    pub contract_id: String,
    pub contract_version: String,
    pub contract_fingerprint: String,
    pub compiled_fingerprint: String,
    pub input_fingerprint: String,
    pub context_fingerprint: String,
    pub target_capability_profile_fingerprint: String,
    pub adapter_id: String,
    pub adapter_version: String,
    pub request_fingerprint: String,
    pub output_fingerprint: String,
    pub output_schema_fingerprint: String,
    pub output_schema_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentEvidence {
    pub adapter_id: String,
    pub adapter_version: String,
    pub source_template_fingerprint: String,
    pub canonical_template_fingerprint: String,
    pub migration_report_fingerprint: String,
    pub render_input_fingerprint: String,
    pub render_report_fingerprint: String,
    pub artifact_fingerprint: String,
    pub approved_baseline_fingerprint: String,
    pub parity_report_fingerprint: String,
    pub normalized_ooxml_equal: bool,
    pub unresolved_references: usize,
}

/// Host-owned PDF conversion evidence recorded from a deterministic fixture or
/// host converter. Payload-free: fingerprints and hashes only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PdfConversionEvidence {
    pub renderer_id: String,
    pub renderer_version: String,
    pub environment_id: String,
    pub artifact_fingerprint: String,
    pub artifact_bytes: u64,
    pub output_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum DocumentOutputKind {
    Docx,
    Html,
    Pdf,
}

/// One document output in a multi-output conformance receipt. Ordering is
/// deterministic: kind, then adapter_id, then artifact_fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentOutputEvidence {
    pub kind: DocumentOutputKind,
    pub adapter_id: String,
    pub adapter_version: String,
    pub source_template_fingerprint: String,
    pub render_input_fingerprint: String,
    pub render_report_fingerprint: String,
    pub artifact_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_conversion: Option<PdfConversionEvidence>,
}

/// A payload-free receipt joining both atomic interactions and DOCX evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConformanceTraceReceipt {
    pub api_version: String,
    #[serde(default = "default_receipt_schema_version")]
    pub receipt_schema_version: String,
    pub package: String,
    pub package_version: String,
    pub package_fingerprint: String,
    pub eval_report_fingerprint: String,
    pub interactions: Vec<InteractionEvidence>,
    pub document: DocumentEvidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<DocumentOutputEvidence>,
}

/// SHA-256 over exact bytes, used for document artifacts that are not semantic JSON.
///
/// # Errors
///
/// Returns an I/O error when `path` cannot be read.
pub fn file_fingerprint(path: &Path) -> std::io::Result<String> {
    Ok(hex::encode(Sha256::digest(fs::read(path)?)))
}

/// Fingerprints a structured report without retaining its payload.
///
/// # Errors
///
/// Returns a serialization error when the report cannot be represented as
/// canonical JSON.
pub fn report_fingerprint(report: &Value) -> Result<String, serde_json::Error> {
    templiqx_contracts::fingerprint(report)
}

/// Sorts multi-output evidence deterministically for receipt assembly.
pub fn sort_document_outputs(outputs: &mut [DocumentOutputEvidence]) {
    outputs.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.adapter_id.cmp(&right.adapter_id))
            .then_with(|| left.artifact_fingerprint.cmp(&right.artifact_fingerprint))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_schema_version_defaults_on_deserialize() {
        let legacy = serde_json::json!({
            "api_version": TRACE_API_VERSION,
            "package": "crm3",
            "package_version": "0.1.0",
            "package_fingerprint": "abc",
            "eval_report_fingerprint": "def",
            "interactions": [],
            "document": {
                "adapter_id": "templiqx-docx-v5",
                "adapter_version": "0.0.0",
                "source_template_fingerprint": "a",
                "canonical_template_fingerprint": "b",
                "migration_report_fingerprint": "c",
                "render_input_fingerprint": "d",
                "render_report_fingerprint": "e",
                "artifact_fingerprint": "f",
                "approved_baseline_fingerprint": "g",
                "parity_report_fingerprint": "h",
                "normalized_ooxml_equal": true,
                "unresolved_references": 0
            }
        });
        let receipt: ConformanceTraceReceipt =
            serde_json::from_value(legacy).expect("legacy receipt deserializes");
        assert_eq!(receipt.receipt_schema_version, RECEIPT_SCHEMA_VERSION);
        assert!(receipt.outputs.is_empty());
    }
}
