//! Local filesystem composition and deterministic conformance adapters.

use fs2::FileExt;
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use templiqx_contracts::{
    AdapterDescriptor, Contract, ExecutionReceipt, ExecutionRequest, PackageManifest, fingerprint,
};
use templiqx_ports::{
    ArtifactWorkspace, DocumentRenderRequest, DocumentRenderResult, DocumentRenderer,
    LegacyImportAdapter, LegacyImportRequest, LegacyImportResult, PackageStore, PortError,
    RuntimeAdapter,
};

#[derive(Debug, Clone)]
pub struct FilesystemPackageStore {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FilesystemArtifactWorkspace {
    root: PathBuf,
}

impl FilesystemPackageStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, PortError> {
        let root = root.as_ref();
        fs::create_dir_all(root).map_err(io_error)?;
        Ok(Self {
            root: root.canonicalize().map_err(io_error)?,
        })
    }
    fn segment(value: &str) -> Result<&str, PortError> {
        if value.is_empty()
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            || matches!(value, "." | "..")
        {
            return Err(PortError::InvalidPath(value.into()));
        }
        Ok(value)
    }
    fn existing_package(&self, package: &str) -> Result<PathBuf, PortError> {
        let path = self.root.join(Self::segment(package)?);
        if fs::symlink_metadata(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    PortError::NotFound(package.into())
                } else {
                    io_error(error)
                }
            })?
            .file_type()
            .is_symlink()
        {
            return Err(PortError::InvalidPath(package.into()));
        }
        let canonical = path.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PortError::NotFound(package.into())
            } else {
                io_error(e)
            }
        })?;
        if !canonical.starts_with(&self.root) || !canonical.is_dir() {
            return Err(PortError::InvalidPath(package.into()));
        }
        Ok(canonical)
    }
    fn contract_path(&self, package: &str, contract: &str) -> Result<PathBuf, PortError> {
        let root = self.existing_package(package)?;
        let dir = root.join("contracts").canonicalize().map_err(io_error)?;
        if !dir.starts_with(&root) || !dir.is_dir() {
            return Err(PortError::InvalidPath("contracts".into()));
        }
        let target = dir.join(format!("{}.yaml", Self::segment(contract)?));
        if target.exists() {
            let canonical = target.canonicalize().map_err(io_error)?;
            if !canonical.starts_with(&dir) || !canonical.is_file() {
                return Err(PortError::InvalidPath(contract.into()));
            }
        }
        Ok(target)
    }
    fn relative_path(relative: &str) -> Result<&Path, PortError> {
        if relative.is_empty()
            || relative.contains('\\')
            || relative
                .split('/')
                .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        {
            return Err(PortError::InvalidPath(relative.into()));
        }
        let relative_path = Path::new(relative);
        if relative_path.is_absolute() {
            return Err(PortError::InvalidPath(relative.into()));
        }
        if !relative_path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            return Err(PortError::InvalidPath(relative.into()));
        }
        Ok(relative_path)
    }
    fn artifact_path(&self, package: &str, relative: &str) -> Result<PathBuf, PortError> {
        let package_root = self.existing_package(package)?;
        let relative_path = Self::relative_path(relative)?;
        let mut candidate = package_root.clone();
        for component in relative_path.components() {
            let std::path::Component::Normal(segment) = component else {
                return Err(PortError::InvalidPath(relative.into()));
            };
            candidate.push(segment);
            let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    PortError::NotFound(relative.into())
                } else {
                    io_error(error)
                }
            })?;
            if metadata.file_type().is_symlink() {
                return Err(PortError::InvalidPath(relative.into()));
            }
        }
        let canonical = candidate.canonicalize().map_err(io_error)?;
        if !canonical.starts_with(&package_root) || !canonical.is_file() {
            return Err(PortError::InvalidPath(relative.into()));
        }
        Ok(canonical)
    }
    fn relative_existing_path(&self, package: &str, path: &Path) -> Result<String, PortError> {
        let package_root = self.existing_package(package)?;
        let canonical = path.canonicalize().map_err(io_error)?;
        if !canonical.starts_with(&package_root) || !canonical.is_file() {
            return Err(PortError::InvalidPath(path.display().to_string()));
        }
        let relative = canonical
            .strip_prefix(&package_root)
            .map_err(|_| PortError::InvalidPath(path.display().to_string()))?;
        let portable = portable_path(relative, path)?;
        // Re-run component-by-component symlink checks on the portable path.
        self.artifact_path(package, &portable)?;
        Ok(portable)
    }
    fn read_yaml<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<T, PortError> {
        let source = fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PortError::NotFound(path.display().to_string())
            } else {
                io_error(e)
            }
        })?;
        serde_yaml_ng::from_str(&source)
            .map_err(|e| PortError::InvalidData(format!("{}: {e}", path.display())))
    }
}

impl FilesystemArtifactWorkspace {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, PortError> {
        let root = root.as_ref();
        fs::create_dir_all(root).map_err(io_error)?;
        Ok(Self {
            root: root.canonicalize().map_err(io_error)?,
        })
    }

    fn selected_root(&self, workspace: Option<&str>) -> Result<PathBuf, PortError> {
        let Some(workspace) = workspace else {
            return Ok(self.root.clone());
        };
        let path = Path::new(workspace);
        if workspace.is_empty()
            || !path.is_absolute()
            || path.components().any(|component| {
                !matches!(
                    component,
                    std::path::Component::RootDir | std::path::Component::Normal(_)
                )
            })
        {
            return Err(PortError::InvalidPath(workspace.into()));
        }
        fs::create_dir_all(path).map_err(io_error)?;
        let canonical = path.canonicalize().map_err(io_error)?;
        if fs::symlink_metadata(&canonical)
            .map_err(io_error)?
            .file_type()
            .is_symlink()
        {
            return Err(PortError::InvalidPath(workspace.into()));
        }
        Ok(canonical)
    }

    fn package_root(&self, package: &str, workspace: Option<&str>) -> Result<PathBuf, PortError> {
        let root = self.selected_root(workspace)?;
        let package_root = root.join(FilesystemPackageStore::segment(package)?);
        fs::create_dir_all(&package_root).map_err(io_error)?;
        let metadata = fs::symlink_metadata(&package_root).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(PortError::InvalidPath(package.into()));
        }
        let canonical = package_root.canonicalize().map_err(io_error)?;
        if !canonical.starts_with(root) {
            return Err(PortError::InvalidPath(package.into()));
        }
        Ok(canonical)
    }

    fn output_path(
        &self,
        package: &str,
        relative: &str,
        workspace: Option<&str>,
    ) -> Result<PathBuf, PortError> {
        let package_root = self.package_root(package, workspace)?;
        let relative_path = FilesystemPackageStore::relative_path(relative)?;
        let file_name = relative_path
            .file_name()
            .ok_or_else(|| PortError::InvalidPath(relative.into()))?;
        let parent_relative = relative_path.parent().unwrap_or_else(|| Path::new(""));
        let mut parent = package_root.clone();
        for component in parent_relative.components() {
            let std::path::Component::Normal(segment) = component else {
                return Err(PortError::InvalidPath(relative.into()));
            };
            parent.push(segment);
            fs::create_dir_all(&parent).map_err(io_error)?;
            let metadata = fs::symlink_metadata(&parent).map_err(io_error)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(PortError::InvalidPath(relative.into()));
            }
        }
        let canonical_parent = parent.canonicalize().map_err(io_error)?;
        if !canonical_parent.starts_with(&package_root) {
            return Err(PortError::InvalidPath(relative.into()));
        }
        let candidate = canonical_parent.join(file_name);
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(PortError::InvalidPath(relative.into()));
                }
                let canonical = candidate.canonicalize().map_err(io_error)?;
                if !canonical.starts_with(&package_root) {
                    return Err(PortError::InvalidPath(relative.into()));
                }
                Ok(canonical)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(candidate),
            Err(error) => Err(io_error(error)),
        }
    }

    fn relative_existing_path(
        &self,
        package: &str,
        path: &Path,
        workspace: Option<&str>,
    ) -> Result<String, PortError> {
        let package_root = self.package_root(package, workspace)?;
        let canonical = path.canonicalize().map_err(io_error)?;
        if !canonical.starts_with(&package_root) || !canonical.is_file() {
            return Err(PortError::InvalidPath(path.display().to_string()));
        }
        let relative = canonical
            .strip_prefix(&package_root)
            .map_err(|_| PortError::InvalidPath(path.display().to_string()))?;
        let portable = portable_path(relative, path)?;
        self.output_path(package, &portable, workspace)?;
        Ok(portable)
    }
}

impl ArtifactWorkspace for FilesystemArtifactWorkspace {
    fn resolve_output_path(
        &self,
        package: &str,
        relative_path: &str,
        workspace: Option<&str>,
    ) -> Result<PathBuf, PortError> {
        self.output_path(package, relative_path, workspace)
    }

    fn relative_artifact_path(
        &self,
        package: &str,
        path: &Path,
        workspace: Option<&str>,
    ) -> Result<String, PortError> {
        self.relative_existing_path(package, path, workspace)
    }
}

impl PackageStore for FilesystemPackageStore {
    fn discover(&self) -> Result<Vec<PackageManifest>, PortError> {
        let mut manifests: Vec<PackageManifest> = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            if entry.file_type().map_err(io_error)?.is_dir() {
                let package = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| PortError::InvalidPath(entry.path().display().to_string()))?;
                match self.artifact_path(&package, "templiqx.yaml") {
                    Ok(manifest) => manifests.push(self.read_yaml(&manifest)?),
                    Err(PortError::NotFound(_)) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        manifests.sort_by(|a, b| a.package.cmp(&b.package).then(a.version.cmp(&b.version)));
        Ok(manifests)
    }
    fn manifest(&self, package: &str) -> Result<PackageManifest, PortError> {
        self.read_yaml(&self.artifact_path(package, "templiqx.yaml")?)
    }
    fn contract(&self, package: &str, contract: &str) -> Result<Contract, PortError> {
        let path = self.contract_path(package, contract)?;
        let source = fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PortError::NotFound(path.display().to_string())
            } else {
                io_error(e)
            }
        })?;
        templiqx_core::parse_contract(&source, Some(&path.display().to_string())).map_err(|d| {
            PortError::InvalidData(
                d.into_iter()
                    .map(|x| x.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
    }
    fn contract_source(&self, package: &str, contract: &str) -> Result<String, PortError> {
        let path = self.contract_path(package, contract)?;
        fs::read_to_string(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                PortError::NotFound(path.display().to_string())
            } else {
                io_error(error)
            }
        })
    }
    fn artifact_bytes(&self, package: &str, relative_path: &str) -> Result<Vec<u8>, PortError> {
        let path = self.artifact_path(package, relative_path)?;
        fs::read(path).map_err(io_error)
    }
    fn resolve_artifact_path(
        &self,
        package: &str,
        relative_path: &str,
    ) -> Result<PathBuf, PortError> {
        self.artifact_path(package, relative_path)
    }
    fn relative_artifact_path(&self, package: &str, path: &Path) -> Result<String, PortError> {
        self.relative_existing_path(package, path)
    }
    fn put_contract(
        &self,
        package: &str,
        contract: &str,
        source: &str,
        expected_fingerprint: Option<&str>,
    ) -> Result<String, PortError> {
        let target = self.contract_path(package, contract)?;
        let package_root = self.existing_package(package)?;
        let lock_path = package_root.join(".templiqx.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(io_error)?;
        lock.lock_exclusive().map_err(io_error)?;
        let actual = if target.exists() {
            Some(
                fingerprint(&self.contract(package, contract)?)
                    .map_err(|e| PortError::InvalidData(e.to_string()))?,
            )
        } else {
            None
        };
        match (expected_fingerprint, actual.as_deref()) {
            (Some(expected), Some(actual)) if expected != actual => {
                return Err(PortError::Conflict(format!(
                    "expected {expected}, found {actual}"
                )));
            }
            (Some(_), None) => return Err(PortError::Conflict("contract does not exist".into())),
            (None, Some(_)) => {
                return Err(PortError::Conflict(
                    "contract already exists; provide expected_fingerprint".into(),
                ));
            }
            _ => {}
        }
        let parsed = templiqx_core::parse_contract(source, Some(contract)).map_err(|d| {
            PortError::InvalidData(
                d.into_iter()
                    .map(|x| x.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        if parsed.id != contract {
            return Err(PortError::InvalidData(format!(
                "contract id '{}' does not match inventory id '{contract}'",
                parsed.id
            )));
        }
        let hash = fingerprint(&parsed).map_err(|e| PortError::InvalidData(e.to_string()))?;
        let mut manifest = self.manifest(package)?;
        let update_manifest = !manifest.contracts.iter().any(|id| id == contract);
        let manifest_target = package_root.join("templiqx.yaml");
        let manifest_tmp = package_root.join(format!("templiqx.yaml.tmp.{}", std::process::id()));
        if update_manifest {
            manifest.contracts.push(contract.to_owned());
            manifest.contracts.sort();
            manifest.contracts.dedup();
            let manifest_source = serde_yaml_ng::to_string(&manifest)
                .map_err(|e| PortError::InvalidData(e.to_string()))?;
            let mut manifest_file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&manifest_tmp)
                .map_err(io_error)?;
            if let Err(error) = manifest_file
                .write_all(manifest_source.as_bytes())
                .and_then(|()| manifest_file.sync_all())
            {
                let _ = fs::remove_file(&manifest_tmp);
                return Err(io_error(error));
            }
        }
        let tmp = target.with_extension(format!("yaml.tmp.{}", std::process::id()));
        let mut file = match OpenOptions::new().create_new(true).write(true).open(&tmp) {
            Ok(file) => file,
            Err(error) => {
                if update_manifest {
                    let _ = fs::remove_file(&manifest_tmp);
                }
                return Err(io_error(error));
            }
        };
        if let Err(error) = file
            .write_all(source.as_bytes())
            .and_then(|()| file.sync_all())
        {
            let _ = fs::remove_file(&tmp);
            if update_manifest {
                let _ = fs::remove_file(&manifest_tmp);
            }
            return Err(io_error(error));
        }
        if let Err(error) = fs::rename(&tmp, &target) {
            let _ = fs::remove_file(&tmp);
            if update_manifest {
                let _ = fs::remove_file(&manifest_tmp);
            }
            return Err(io_error(error));
        }
        if update_manifest && let Err(error) = fs::rename(&manifest_tmp, manifest_target) {
            // A new contract is not visible unless its manifest update is also
            // committed. Roll back the just-created contract on failure so a
            // retry does not encounter an orphan/CAS conflict.
            let rollback = fs::remove_file(&target);
            let _ = fs::remove_file(&manifest_tmp);
            return Err(match rollback {
                Ok(()) => io_error(error),
                Err(rollback) => PortError::Io(format!(
                    "manifest commit failed: {error}; contract rollback failed: {rollback}"
                )),
            });
        }
        FileExt::unlock(&lock).map_err(io_error)?;
        Ok(hash)
    }
}

fn io_error(e: std::io::Error) -> PortError {
    PortError::Io(e.to_string())
}

fn portable_path(relative: &Path, original: &Path) -> Result<String, PortError> {
    relative
        .components()
        .map(|component| match component {
            std::path::Component::Normal(segment) => segment
                .to_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| PortError::InvalidPath(original.display().to_string())),
            _ => Err(PortError::InvalidPath(original.display().to_string())),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|segments| segments.join("/"))
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicFakeRuntime;
impl RuntimeAdapter for DeterministicFakeRuntime {
    fn descriptor(&self) -> AdapterDescriptor {
        AdapterDescriptor {
            id: "templiqx.fake".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            capabilities: vec!["structured_output".into(), "text".into()],
        }
    }
    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError> {
        let output = request
            .fixture_output
            .clone()
            .ok_or_else(|| PortError::InvalidData("fake runtime requires fixture_output".into()))?;
        let valid =
            templiqx_core::validate_output(&request.interaction.output_schema, &output).is_empty();
        Ok(ExecutionReceipt {
            adapter: self.descriptor(),
            request_fingerprint: fingerprint(&request.interaction)
                .map_err(|e| PortError::InvalidData(e.to_string()))?,
            output_fingerprint: fingerprint(&output)
                .map_err(|e| PortError::InvalidData(e.to_string()))?,
            output,
            output_schema_valid: valid,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct UnsupportedLegacyAdapter;
impl LegacyImportAdapter for UnsupportedLegacyAdapter {
    fn migrate(&self, request: &LegacyImportRequest) -> Result<LegacyImportResult, PortError> {
        Err(PortError::Unsupported(format!(
            "legacy dialect '{}' adapter is not installed",
            request.dialect
        )))
    }
}
#[derive(Debug, Clone, Default)]
pub struct UnsupportedDocumentRenderer;
impl DocumentRenderer for UnsupportedDocumentRenderer {
    fn render_document(
        &self,
        _request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        Err(PortError::Unsupported(
            "document renderer adapter is not installed".into(),
        ))
    }
}

pub type CoreOnlyService = templiqx_application::TempliqxService<
    FilesystemPackageStore,
    FilesystemArtifactWorkspace,
    DeterministicFakeRuntime,
    UnsupportedLegacyAdapter,
    UnsupportedDocumentRenderer,
>;
pub type LocalService = templiqx_application::TempliqxService<
    FilesystemPackageStore,
    FilesystemArtifactWorkspace,
    DeterministicFakeRuntime,
    templiqx_docx_v5::DocxV5Adapter,
    templiqx_docx_v5::DocxV5Adapter,
>;

pub fn compose_core(root: impl AsRef<Path>) -> Result<CoreOnlyService, PortError> {
    let root = root.as_ref();
    Ok(templiqx_application::TempliqxService::new(
        FilesystemPackageStore::new(root)?,
        FilesystemArtifactWorkspace::new(root.join(".templiqx-workspace"))?,
        DeterministicFakeRuntime,
        UnsupportedLegacyAdapter,
        UnsupportedDocumentRenderer,
    ))
}

pub fn compose(root: impl AsRef<Path>) -> Result<LocalService, PortError> {
    let root = root.as_ref();
    compose_with_workspace(root, root.join(".templiqx-workspace"))
}

pub fn compose_with_workspace(
    root: impl AsRef<Path>,
    workspace: impl AsRef<Path>,
) -> Result<LocalService, PortError> {
    Ok(templiqx_application::TempliqxService::new(
        FilesystemPackageStore::new(root)?,
        FilesystemArtifactWorkspace::new(workspace)?,
        DeterministicFakeRuntime,
        templiqx_docx_v5::DocxV5Adapter::default(),
        templiqx_docx_v5::DocxV5Adapter::default(),
    ))
}

pub fn create_package(
    root: impl AsRef<Path>,
    name: &str,
    version: &str,
) -> Result<PathBuf, PortError> {
    FilesystemPackageStore::segment(name)?;
    let package = root.as_ref().join(name);
    for directory in [
        "contracts",
        "components",
        "evals",
        "migrations",
        "templates",
    ] {
        fs::create_dir_all(package.join(directory)).map_err(io_error)?;
    }
    let manifest = PackageManifest {
        api_version: templiqx_contracts::API_VERSION.into(),
        package: name.into(),
        version: version.into(),
        description: String::new(),
        contracts: vec![],
        components: vec![],
        evals: vec![],
        migrations: vec![],
        templates: vec![],
        provenance: Default::default(),
    };
    let yaml =
        serde_yaml_ng::to_string(&manifest).map_err(|e| PortError::InvalidData(e.to_string()))?;
    fs::write(package.join("templiqx.yaml"), yaml).map_err(io_error)?;
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_and_symlink_escape() {
        let temp = tempfile::tempdir().unwrap();
        let store = FilesystemPackageStore::new(temp.path()).unwrap();
        assert!(matches!(
            store.manifest("../etc"),
            Err(PortError::InvalidPath(_))
        ));
        create_package(temp.path(), "safe", "0.1.0").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/tmp", temp.path().join("safe/contracts/escape.yaml"))
                .unwrap();
            assert!(matches!(
                store.contract("safe", "escape"),
                Err(PortError::InvalidPath(_))
            ));
        }
    }
}
