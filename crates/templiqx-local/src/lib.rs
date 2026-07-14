//! Local filesystem composition and deterministic conformance adapters.

use fs2::FileExt;
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use templiqx_contracts::{
    AdapterDescriptor, Contract, ExecutionReceipt, ExecutionRequest, PackageIdentity,
    PackageManifest, PackageSignature, fingerprint, fingerprint_bytes,
};
use templiqx_ports::{
    ArtifactWorkspace, DocumentInspectionRequest, DocumentInspectionResult, DocumentInspector,
    DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, LegacyImportAdapter,
    LegacyImportRequest, LegacyImportResult, PackageStore, PortError, RuntimeAdapter,
    WorkspaceOutputLease,
};

#[derive(Debug, Clone)]
pub struct FilesystemPackageStore {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FilesystemArtifactWorkspace {
    root: PathBuf,
}

struct FilesystemWorkspaceOutputLease {
    path: PathBuf,
    _lock: fs::File,
}

impl WorkspaceOutputLease for FilesystemWorkspaceOutputLease {
    fn path(&self) -> &Path {
        &self.path
    }
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

    fn package_lock_file(
        &self,
        package: &str,
        workspace: Option<&str>,
    ) -> Result<fs::File, PortError> {
        let selected_root = self.selected_root(workspace)?;
        let lock_dir = selected_root.join(".templiqx-workspace-locks");
        fs::create_dir_all(&lock_dir).map_err(io_error)?;
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_dir.join(format!(
                "{}.lock",
                FilesystemPackageStore::segment(package)?
            )))
            .map_err(io_error)
    }

    fn package_lock(&self, package: &str, workspace: Option<&str>) -> Result<fs::File, PortError> {
        let lock = self.package_lock_file(package, workspace)?;
        lock.lock_exclusive().map_err(io_error)?;
        Ok(lock)
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

    fn validated_prefix(prefix: Option<&str>) -> Result<Option<String>, PortError> {
        let Some(prefix) = prefix else {
            return Ok(None);
        };
        let trimmed = prefix.strip_suffix('/').unwrap_or(prefix);
        if trimmed.is_empty() {
            return Ok(None);
        }
        FilesystemPackageStore::relative_path(trimmed)?;
        Ok(Some(trimmed.to_owned()))
    }

    fn existing_artifact_path(
        &self,
        package: &str,
        relative: &str,
        workspace: Option<&str>,
    ) -> Result<PathBuf, PortError> {
        let package_root = self.package_root(package, workspace)?;
        let relative_path = FilesystemPackageStore::relative_path(relative)?;
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

    fn collect_artifacts(
        dir: &Path,
        package_root: &Path,
        prefix: Option<&str>,
        out: &mut Vec<(PathBuf, u64)>,
    ) -> Result<(), PortError> {
        let mut entries = fs::read_dir(dir)
            .map_err(io_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io_error)?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                Self::collect_artifacts(&path, package_root, prefix, out)?;
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let relative = path
                .strip_prefix(package_root)
                .map_err(|_| PortError::InvalidPath(path.display().to_string()))?;
            let portable = portable_path(relative, &path)?;
            let included = match prefix {
                None => true,
                Some(prefix) => portable == prefix || portable.starts_with(&format!("{prefix}/")),
            };
            if included {
                out.push((path, metadata.len()));
            }
        }
        Ok(())
    }
}

impl ArtifactWorkspace for FilesystemArtifactWorkspace {
    fn lease_output_path(
        &self,
        package: &str,
        relative_path: &str,
        workspace: Option<&str>,
    ) -> Result<Box<dyn WorkspaceOutputLease>, PortError> {
        let lock = self.package_lock(package, workspace)?;
        let path = self.output_path(package, relative_path, workspace)?;
        Ok(Box::new(FilesystemWorkspaceOutputLease {
            path,
            _lock: lock,
        }))
    }

    fn relative_artifact_path(
        &self,
        package: &str,
        path: &Path,
        workspace: Option<&str>,
    ) -> Result<String, PortError> {
        self.relative_existing_path(package, path, workspace)
    }

    fn list_artifacts(
        &self,
        package: &str,
        workspace: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Vec<(PathBuf, u64)>, PortError> {
        let lock = self.package_lock_file(package, workspace)?;
        FileExt::lock_shared(&lock).map_err(io_error)?;
        let prefix = Self::validated_prefix(prefix)?;
        let package_root = self.package_root(package, workspace)?;
        if !package_root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        Self::collect_artifacts(&package_root, &package_root, prefix.as_deref(), &mut out)?;
        out.sort_by(|a, b| a.0.cmp(&b.0));
        FileExt::unlock(&lock).map_err(io_error)?;
        Ok(out)
    }

    fn read_artifact(
        &self,
        package: &str,
        relative_path: &str,
        workspace: Option<&str>,
    ) -> Result<Vec<u8>, PortError> {
        let lock = self.package_lock_file(package, workspace)?;
        FileExt::lock_shared(&lock).map_err(io_error)?;
        let path = self.existing_artifact_path(package, relative_path, workspace)?;
        let bytes = fs::read(path).map_err(io_error)?;
        FileExt::unlock(&lock).map_err(io_error)?;
        Ok(bytes)
    }

    fn delete_artifact(
        &self,
        package: &str,
        relative_path: &str,
        workspace: Option<&str>,
        expected_fingerprint: &str,
    ) -> Result<String, PortError> {
        let lock = self.package_lock(package, workspace)?;
        let path = self.existing_artifact_path(package, relative_path, workspace)?;
        let bytes = fs::read(&path).map_err(io_error)?;
        let actual = templiqx_contracts::fingerprint_bytes(&bytes);
        if actual != expected_fingerprint {
            return Err(PortError::Conflict(format!(
                "expected {expected_fingerprint}, found {actual}"
            )));
        }
        fs::remove_file(path).map_err(io_error)?;
        FileExt::unlock(&lock).map_err(io_error)?;
        Ok(actual)
    }
}

impl PackageStore for FilesystemPackageStore {
    fn package_identity(&self, package: &str) -> Result<PackageIdentity, PortError> {
        let mut manifest = self.manifest(package)?;
        manifest.signatures.clear();
        manifest.contracts.sort();
        manifest.components.sort();
        manifest.evals.sort();
        manifest.migrations.sort();
        manifest.templates.sort();
        manifest.translations.sort();
        let mut paths = manifest
            .contracts
            .iter()
            .map(|id| format!("contracts/{id}.yaml"))
            .collect::<Vec<_>>();
        paths.extend(manifest.components.iter().cloned());
        paths.extend(manifest.evals.iter().cloned());
        paths.extend(manifest.migrations.iter().cloned());
        paths.extend(manifest.templates.iter().cloned());
        paths.extend(
            manifest
                .translations
                .iter()
                .map(|locale| format!("translations/{locale}.yaml")),
        );
        paths.sort();
        paths.dedup();
        let mut artifacts = std::collections::BTreeMap::new();
        for path in paths {
            artifacts.insert(
                path.clone(),
                fingerprint_bytes(&self.artifact_bytes(package, &path)?),
            );
        }
        match self.artifact_bytes(package, "templiqx.lock") {
            Ok(bytes) => {
                artifacts.insert("templiqx.lock".to_owned(), fingerprint_bytes(&bytes));
            }
            Err(PortError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
        Ok(PackageIdentity {
            manifest,
            artifacts,
        })
    }

    fn create_package(&self, name: &str, version: &str) -> Result<PackageManifest, PortError> {
        match self.existing_package(name) {
            Ok(_) => {
                return Err(PortError::Conflict(format!(
                    "package '{name}' already exists"
                )));
            }
            Err(PortError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
        create_package(&self.root, name, version)?;
        self.manifest(name)
    }

    fn update_package(
        &self,
        package: &str,
        version: Option<&str>,
        description: Option<&str>,
        expected_fingerprint: &str,
    ) -> Result<PackageManifest, PortError> {
        let package_root = self.existing_package(package)?;
        let lock = package_lock(&self.root, package)?;
        lock.lock_exclusive().map_err(io_error)?;
        let mut manifest = self.manifest(package)?;
        let actual = fingerprint(&manifest).map_err(|e| PortError::InvalidData(e.to_string()))?;
        if actual != expected_fingerprint {
            FileExt::unlock(&lock).map_err(io_error)?;
            return Err(PortError::Conflict(format!(
                "expected {expected_fingerprint}, found {actual}"
            )));
        }
        if let Some(version) = version {
            manifest.version = version.to_owned();
        }
        if let Some(description) = description {
            manifest.description = description.to_owned();
        }
        manifest.signatures.clear();
        write_manifest(&package_root, &manifest)?;
        FileExt::unlock(&lock).map_err(io_error)?;
        Ok(manifest)
    }

    fn delete_package(
        &self,
        package: &str,
        expected_fingerprint: &str,
    ) -> Result<String, PortError> {
        let package_root = self.existing_package(package)?;
        let lock = package_lock(&self.root, package)?;
        lock.lock_exclusive().map_err(io_error)?;
        let manifest = self.manifest(package)?;
        let actual = fingerprint(&manifest).map_err(|e| PortError::InvalidData(e.to_string()))?;
        if actual != expected_fingerprint {
            FileExt::unlock(&lock).map_err(io_error)?;
            return Err(PortError::Conflict(format!(
                "expected {expected_fingerprint}, found {actual}"
            )));
        }

        for candidate in self.discover()? {
            let lock_references_package = self
                .artifact_bytes(&candidate.package, "templiqx.lock")
                .ok()
                .and_then(|bytes| {
                    serde_yaml_ng::from_slice::<templiqx_contracts::PackageLock>(&bytes).ok()
                })
                .is_some_and(|lock| lock.dependencies.contains_key(package));
            if candidate.package != package
                && (candidate.dependencies.contains_key(package) || lock_references_package)
            {
                FileExt::unlock(&lock).map_err(io_error)?;
                return Err(PortError::Conflict(format!(
                    "package '{}' depends on '{package}'",
                    candidate.package
                )));
            }
        }

        let mut tracked = std::collections::BTreeSet::from([
            "templiqx.yaml".to_owned(),
            "templiqx.lock".to_owned(),
            ".templiqx.lock".to_owned(),
        ]);
        tracked.extend(
            manifest
                .contracts
                .iter()
                .map(|id| format!("contracts/{id}.yaml")),
        );
        tracked.extend(manifest.components.iter().cloned());
        tracked.extend(manifest.evals.iter().cloned());
        tracked.extend(manifest.migrations.iter().cloned());
        tracked.extend(manifest.templates.iter().cloned());
        let files = package_files(&package_root, &package_root)?;
        if let Some(untracked) = files.iter().find(|path| !tracked.contains(*path)) {
            FileExt::unlock(&lock).map_err(io_error)?;
            return Err(PortError::Conflict(format!(
                "untracked content would be lost: {untracked}"
            )));
        }
        fs::remove_dir_all(&package_root).map_err(io_error)?;
        Ok(actual)
    }

    fn attach_package_signature(
        &self,
        package: &str,
        signature: PackageSignature,
        expected_fingerprint: &str,
        expected_identity_fingerprint: &str,
    ) -> Result<PackageManifest, PortError> {
        let package_root = self.existing_package(package)?;
        let lock = package_lock(&self.root, package)?;
        lock.lock_exclusive().map_err(io_error)?;
        let mut manifest = self.manifest(package)?;
        let actual = fingerprint(&manifest).map_err(|e| PortError::InvalidData(e.to_string()))?;
        if actual != expected_fingerprint {
            FileExt::unlock(&lock).map_err(io_error)?;
            return Err(PortError::Conflict(format!(
                "expected {expected_fingerprint}, found {actual}"
            )));
        }
        let identity = self.package_identity(package)?;
        let actual_identity =
            fingerprint(&identity).map_err(|e| PortError::InvalidData(e.to_string()))?;
        if actual_identity != expected_identity_fingerprint {
            FileExt::unlock(&lock).map_err(io_error)?;
            return Err(PortError::Conflict(format!(
                "expected package identity {expected_identity_fingerprint}, found {actual_identity}"
            )));
        }
        manifest.signatures.retain(|existing| {
            existing.key_id != signature.key_id || existing.algorithm != signature.algorithm
        });
        manifest.signatures.push(signature);
        manifest.signatures.sort_by(|left, right| {
            left.key_id
                .cmp(&right.key_id)
                .then(left.algorithm.cmp(&right.algorithm))
        });
        write_manifest(&package_root, &manifest)?;
        FileExt::unlock(&lock).map_err(io_error)?;
        Ok(manifest)
    }

    fn delete_contract(
        &self,
        package: &str,
        contract: &str,
        expected_fingerprint: &str,
    ) -> Result<String, PortError> {
        let target = self.contract_path(package, contract)?;
        let package_root = self.existing_package(package)?;
        let lock = package_lock(&self.root, package)?;
        lock.lock_exclusive().map_err(io_error)?;

        let parsed = self.contract(package, contract)?;
        let actual = fingerprint(&parsed).map_err(|e| PortError::InvalidData(e.to_string()))?;
        if expected_fingerprint != actual {
            FileExt::unlock(&lock).map_err(io_error)?;
            return Err(PortError::Conflict(format!(
                "expected {expected_fingerprint}, found {actual}"
            )));
        }

        let mut manifest = self.manifest(package)?;
        let had_entry = manifest.contracts.iter().any(|id| id == contract);
        manifest.contracts.retain(|id| id != contract);
        manifest.signatures.clear();

        let manifest_target = package_root.join("templiqx.yaml");
        let manifest_tmp = package_root.join(format!("templiqx.yaml.tmp.{}", std::process::id()));
        if had_entry {
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
                FileExt::unlock(&lock).map_err(io_error)?;
                return Err(io_error(error));
            }
        }

        if let Err(error) = fs::remove_file(&target) {
            if had_entry {
                let _ = fs::remove_file(&manifest_tmp);
            }
            FileExt::unlock(&lock).map_err(io_error)?;
            return Err(if error.kind() == std::io::ErrorKind::NotFound {
                PortError::NotFound(target.display().to_string())
            } else {
                io_error(error)
            });
        }

        if had_entry && let Err(error) = fs::rename(&manifest_tmp, manifest_target) {
            let _ = fs::remove_file(&manifest_tmp);
            FileExt::unlock(&lock).map_err(io_error)?;
            return Err(io_error(error));
        }

        FileExt::unlock(&lock).map_err(io_error)?;
        Ok(actual)
    }

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
        let lock = package_lock(&self.root, package)?;
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
        let add_to_inventory = !manifest.contracts.iter().any(|id| id == contract);
        let update_manifest = add_to_inventory || !manifest.signatures.is_empty();
        manifest.signatures.clear();
        let manifest_target = package_root.join("templiqx.yaml");
        let manifest_tmp = package_root.join(format!("templiqx.yaml.tmp.{}", std::process::id()));
        if add_to_inventory {
            manifest.contracts.push(contract.to_owned());
            manifest.contracts.sort();
            manifest.contracts.dedup();
        }
        if update_manifest {
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

fn package_lock(root: &Path, package: &str) -> Result<std::fs::File, PortError> {
    let lock_dir = root.join(".templiqx-package-locks");
    fs::create_dir_all(&lock_dir).map_err(io_error)?;
    let metadata = fs::symlink_metadata(&lock_dir).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PortError::InvalidPath(lock_dir.display().to_string()));
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_dir.join(format!(
            "{}.lock",
            FilesystemPackageStore::segment(package)?
        )))
        .map_err(io_error)
}

fn write_manifest(package_root: &Path, manifest: &PackageManifest) -> Result<(), PortError> {
    let target = package_root.join("templiqx.yaml");
    let temporary = package_root.join(format!("templiqx.yaml.tmp.{}", std::process::id()));
    let source = serde_yaml_ng::to_string(manifest)
        .map_err(|error| PortError::InvalidData(error.to_string()))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(io_error)?;
    if let Err(error) = file
        .write_all(source.as_bytes())
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temporary, &target))
    {
        let _ = fs::remove_file(&temporary);
        return Err(io_error(error));
    }
    Ok(())
}

fn package_files(directory: &Path, root: &Path) -> Result<Vec<String>, PortError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() {
            return Err(PortError::InvalidPath(path.display().to_string()));
        }
        if metadata.is_dir() {
            files.extend(package_files(&path, root)?);
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| PortError::InvalidPath(path.display().to_string()))?;
            files.push(portable_path(relative, &path)?);
        }
    }
    Ok(files)
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

#[derive(Debug, Clone, Default)]
pub struct UnsupportedDocumentInspector;
impl DocumentInspector for UnsupportedDocumentInspector {
    fn inspect_document(
        &self,
        _request: &DocumentInspectionRequest,
    ) -> Result<DocumentInspectionResult, PortError> {
        Err(PortError::Unsupported(
            "document inspector adapter is not installed".into(),
        ))
    }
}

pub type CoreOnlyService = templiqx_application::TempliqxService<
    FilesystemPackageStore,
    FilesystemArtifactWorkspace,
    DeterministicFakeRuntime,
    UnsupportedLegacyAdapter,
    UnsupportedDocumentRenderer,
    UnsupportedDocumentInspector,
>;
pub type LocalService = templiqx_application::TempliqxService<
    FilesystemPackageStore,
    FilesystemArtifactWorkspace,
    DeterministicFakeRuntime,
    templiqx_docx_v5::DocxV5Adapter,
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
        UnsupportedDocumentInspector,
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
        templiqx_docx_v5::DocxV5Adapter::default(),
    ))
}

pub fn create_package(
    root: impl AsRef<Path>,
    name: &str,
    version: &str,
) -> Result<PathBuf, PortError> {
    FilesystemPackageStore::segment(name)?;
    fs::create_dir_all(root.as_ref()).map_err(io_error)?;
    let root = root.as_ref().canonicalize().map_err(io_error)?;
    let lock = package_lock(&root, name)?;
    lock.lock_exclusive().map_err(io_error)?;
    let package = root.join(name);
    match fs::symlink_metadata(&package) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(PortError::InvalidPath(package.display().to_string()));
        }
        Ok(_) => {
            return Err(PortError::Conflict(format!(
                "package '{name}' already exists"
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error)),
    }
    for directory in [
        "contracts",
        "components",
        "evals",
        "migrations",
        "templates",
        "translations",
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
        translations: vec![],
        provenance: Default::default(),
        signatures: vec![],
        dependencies: Default::default(),
        tool_contracts: Default::default(),
    };
    let yaml =
        serde_yaml_ng::to_string(&manifest).map_err(|e| PortError::InvalidData(e.to_string()))?;
    fs::write(package.join("templiqx.yaml"), yaml).map_err(io_error)?;
    FileExt::unlock(&lock).map_err(io_error)?;
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
