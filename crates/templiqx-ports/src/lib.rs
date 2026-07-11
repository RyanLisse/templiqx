//! Host-facing ports. Implementations belong in adapters, never the core.

use serde_json::Value;
use std::path::{Path, PathBuf};
use templiqx_contracts::{
    AdapterDescriptor, Contract, ExecutionReceipt, ExecutionRequest, PackageManifest,
};

#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("invalid data: {0}")]
    InvalidData(String),
}

pub trait PackageStore: Send + Sync {
    fn discover(&self) -> Result<Vec<PackageManifest>, PortError>;
    fn manifest(&self, package: &str) -> Result<PackageManifest, PortError>;
    fn contract(&self, package: &str, contract: &str) -> Result<Contract, PortError>;
    fn contract_source(&self, package: &str, contract: &str) -> Result<String, PortError>;
    /// Read an exact manifest-listed artifact without following symlinks or
    /// allowing the path to escape the package root.
    fn artifact_bytes(&self, package: &str, relative_path: &str) -> Result<Vec<u8>, PortError>;
    /// Resolve an existing package-relative regular file without following
    /// symlinks or allowing the path to escape the package root.
    fn resolve_artifact_path(
        &self,
        package: &str,
        relative_path: &str,
    ) -> Result<PathBuf, PortError>;
    /// Resolve a package-relative output file whose parent already exists,
    /// rejecting absolute paths, traversal, backslashes, and symlinks.
    fn resolve_output_path(&self, package: &str, relative_path: &str)
    -> Result<PathBuf, PortError>;
    /// Convert an existing adapter-produced artifact back to its portable,
    /// package-relative path after verifying it remains confined.
    fn relative_artifact_path(&self, package: &str, path: &Path) -> Result<String, PortError>;
    fn put_contract(
        &self,
        package: &str,
        contract: &str,
        source: &str,
        expected_fingerprint: Option<&str>,
    ) -> Result<String, PortError>;
}

pub trait RuntimeAdapter: Send + Sync {
    fn descriptor(&self) -> AdapterDescriptor;
    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError>;
}

#[derive(Debug, Clone)]
pub struct LegacyImportRequest {
    pub dialect: String,
    pub source: PathBuf,
    pub aliases: Value,
}
#[derive(Debug, Clone)]
pub struct LegacyImportResult {
    pub report: Value,
    pub canonical_template: Option<PathBuf>,
}
pub trait LegacyImportAdapter: Send + Sync {
    fn migrate(&self, request: &LegacyImportRequest) -> Result<LegacyImportResult, PortError>;
}

#[derive(Debug, Clone)]
pub struct DocumentRenderRequest {
    pub template: PathBuf,
    pub data: Value,
    pub output: PathBuf,
}
#[derive(Debug, Clone)]
pub struct DocumentRenderResult {
    pub artifact: PathBuf,
    pub report: Value,
}
pub trait DocumentRenderer: Send + Sync {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError>;
}
