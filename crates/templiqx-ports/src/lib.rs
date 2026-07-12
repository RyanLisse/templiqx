//! Host-facing ports. Implementations belong in adapters, never core.

use serde_json::Value;
use std::path::{Path, PathBuf};
use templiqx_contracts::{
    AdapterDescriptor, Contract, ExecutionReceipt, ExecutionRequest, PackageManifest, StreamEvent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFailure {
    pub code: RuntimeFailureCode,
    pub adapter_id: String,
    pub adapter_version: String,
    pub scenario_id: Option<String>,
    pub retry_after_ms: Option<u64>,
    pub fingerprint: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFailureCode {
    Timeout,
    RateLimited,
    Unavailable,
    InvalidResponse,
    Permanent,
    HostRetryExhausted,
}

impl RuntimeFailureCode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "TQX_RUNTIME_TIMEOUT",
            Self::RateLimited => "TQX_RUNTIME_RATE_LIMITED",
            Self::Unavailable => "TQX_RUNTIME_UNAVAILABLE",
            Self::InvalidResponse => "TQX_RUNTIME_INVALID_RESPONSE",
            Self::Permanent => "TQX_RUNTIME_PERMANENT",
            Self::HostRetryExhausted => "TQX_HOST_RETRY_EXHAUSTED",
        }
    }
}

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
    #[error("runtime failure {code}: {detail}")]
    RuntimeFailure {
        code: &'static str,
        detail: String,
        failure: Box<RuntimeFailure>,
    },
}

impl From<RuntimeFailure> for PortError {
    fn from(failure: RuntimeFailure) -> Self {
        Self::RuntimeFailure {
            code: failure.code.as_str(),
            detail: failure.detail.clone(),
            failure: Box::new(failure),
        }
    }
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

    /// Convert an existing package artifact back to its portable,
    /// package-relative path after verifying it remains confined.
    fn relative_artifact_path(&self, package: &str, path: &Path) -> Result<String, PortError>;

    fn put_contract(
        &self,
        package: &str,
        contract: &str,
        source: &str,
        expected_fingerprint: Option<&str>,
    ) -> Result<String, PortError>;

    fn create_package(&self, name: &str, version: &str) -> Result<PackageManifest, PortError>;

    fn delete_contract(
        &self,
        package: &str,
        contract: &str,
        expected_fingerprint: &str,
    ) -> Result<String, PortError>;
}

pub trait ArtifactWorkspace: Send + Sync {
    /// Resolve writable artifact output outside package identity, rejecting
    /// absolute paths, traversal, backslashes, and symlink escapes.
    fn resolve_output_path(
        &self,
        package: &str,
        relative_path: &str,
        workspace: Option<&str>,
    ) -> Result<PathBuf, PortError>;

    /// Convert adapter-produced workspace artifact back to its portable path
    /// after verifying it remains confined.
    fn relative_artifact_path(
        &self,
        package: &str,
        path: &Path,
        workspace: Option<&str>,
    ) -> Result<String, PortError>;

    /// List existing files under a package's workspace root, optionally
    /// filtered to those whose portable relative path starts with `prefix`.
    fn list_artifacts(
        &self,
        package: &str,
        workspace: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Vec<(PathBuf, u64)>, PortError>;

    /// Read the bytes of an existing workspace artifact using the same
    /// confinement rules as [`ArtifactWorkspace::resolve_output_path`].
    fn read_artifact(
        &self,
        package: &str,
        relative_path: &str,
        workspace: Option<&str>,
    ) -> Result<Vec<u8>, PortError>;
}

pub trait RuntimeAdapter: Send + Sync {
    fn descriptor(&self) -> AdapterDescriptor;
    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError>;

    /// Optional streaming path. The default forwards to `execute` and emits the
    /// whole receipt as a single terminal `Complete` event, so adapters that do
    /// not stream need no code change. Overriding adapters must still make the
    /// terminal `Complete` receipt identical to what `execute` would produce.
    fn execute_streaming(
        &self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<ExecutionReceipt, PortError> {
        let receipt = self.execute(request)?;
        sink(StreamEvent::Complete(receipt.clone()));
        Ok(receipt)
    }
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
