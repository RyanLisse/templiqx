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

/// A payload-free receipt joining both atomic interactions and DOCX evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConformanceTraceReceipt {
    pub api_version: String,
    pub package: String,
    pub package_version: String,
    pub package_fingerprint: String,
    pub eval_report_fingerprint: String,
    pub interactions: Vec<InteractionEvidence>,
    pub document: DocumentEvidence,
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
