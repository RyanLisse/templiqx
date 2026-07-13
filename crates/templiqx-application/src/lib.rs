//! Actor-neutral atomic Templiqx capabilities used by Rust, CLI and MCP.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use templiqx_contracts::{
    ArtifactContent, ContentEncoding, Contract, ContractDiff, ContractSummary, Diagnostic,
    ExecutionRequest, Explanation, OperationEnvelope, PackageIdentity, PackageManifest,
    PackageSignature, PackageTrustReport, RenderRequest, Severity, TestCaseResult, TestReport,
    WorkspaceArtifact, fingerprint, fingerprint_bytes,
};
use templiqx_ports::{
    ArtifactWorkspace, DocumentRenderRequest as AdapterDocumentRenderRequest, DocumentRenderer,
    LegacyImportAdapter, LegacyImportRequest as AdapterLegacyImportRequest, PackageStore,
    PortError, RuntimeAdapter,
};

/// Actor-neutral request for migrating one package-confined legacy artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MigrateLegacyRequest {
    pub package: String,
    pub dialect: String,
    /// Portable path relative to the selected package root.
    pub source: String,
    pub aliases: Value,
}

/// Portable migration outcome returned identically to Rust, CLI, and MCP.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MigrationResult {
    pub report: Value,
    pub canonical_template: Option<String>,
}

/// Actor-neutral request for rendering one package-confined document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RenderDocumentRequest {
    pub package: String,
    /// Portable input path relative to the selected package root.
    pub template: String,
    pub data: Value,
    /// Portable output path relative to the selected workspace root.
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

/// Portable document-render result returned identically on every surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RenderDocumentResult {
    pub artifact: String,
    pub report: Value,
}

/// Actor-neutral request to bootstrap an empty portable package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreatePackageRequest {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpdatePackageRequest {
    pub package: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub expected_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeletePackageRequest {
    pub package: String,
    pub expected_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SignPackageRequest {
    pub package: String,
    pub key_id: String,
    pub expected_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VerifyPackageTrustRequest {
    pub package: String,
    #[serde(default)]
    pub strict: bool,
}

/// Actor-neutral request to delete one contract with CAS safety.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeleteContractRequest {
    pub package: String,
    pub contract: String,
    pub expected_fingerprint: String,
}

/// Actor-neutral request to list files under one package's workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ListWorkspaceArtifactsRequest {
    pub package: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

/// Actor-neutral request to read one workspace artifact's bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReadArtifactRequest {
    pub package: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeleteWorkspaceArtifactRequest {
    pub package: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    pub expected_fingerprint: String,
}

/// One addressable `(contract, fixture)` pair, as enumerated by `list_evals`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvalCase {
    pub contract_id: String,
    pub fixture_id: String,
}

pub const CAPABILITY_CATALOG: &[&str] = &[
    "catalog",
    "discover_packages",
    "create_package",
    "update_package",
    "delete_package",
    "export_package_identity",
    "sign_package",
    "verify_package_trust",
    "inspect_contract",
    "put_contract",
    "delete_contract",
    "validate_contract",
    "validate_package",
    "compile_contract",
    "render_contract",
    "execute_contract",
    "migrate_legacy",
    "render_document",
    "list_workspace_artifacts",
    "read_artifact",
    "delete_workspace_artifact",
    "test_package",
    "list_evals",
    "run_eval",
    "diff_contract",
    "explain_contract",
];

pub struct TempliqxService<S, W, R, L, D> {
    store: S,
    workspace: W,
    runtime: R,
    legacy: L,
    documents: D,
}

impl<S, W, R, L, D> TempliqxService<S, W, R, L, D>
where
    S: PackageStore,
    W: ArtifactWorkspace,
    R: RuntimeAdapter,
    L: LegacyImportAdapter,
    D: DocumentRenderer,
{
    pub fn new(store: S, workspace: W, runtime: R, legacy: L, documents: D) -> Self {
        Self {
            store,
            workspace,
            runtime,
            legacy,
            documents,
        }
    }

    fn load_contract(&self, package: &str, contract: &str) -> Result<Contract, Vec<Diagnostic>> {
        let source = self
            .store
            .contract_source(package, contract)
            .map_err(|error| vec![port_diagnostic(error)])?;
        let mut parsed = templiqx_core::parse_contract(
            &source,
            Some(&format!("{package}/contracts/{contract}.yaml")),
        )?;
        // U2 (plan 001): inline any tool-contract references before the caller
        // validates or compiles, so downstream sees fully-resolved bounded schemas.
        if let Ok(manifest) = self.store.manifest(package)
            && !manifest.tool_contracts.is_empty()
        {
            let diagnostics =
                templiqx_core::resolve_tool_contract_refs(&mut parsed, &manifest.tool_contracts);
            if diagnostics.iter().any(|d| d.severity == Severity::Error) {
                return Err(diagnostics);
            }
        }
        // U4 (plan 001): expand `include` nodes (package-relative, optionally
        // cross-package) before validation/compilation. The portable core never
        // reads files, so expansion, cycle detection, and path confinement live
        // here. After this, the content tree contains no Include nodes.
        for message in &mut parsed.messages {
            let content = std::mem::take(&mut message.content);
            message.content = self.expand_includes(package, content, &mut Vec::new())?;
        }
        for definition in parsed.components.values_mut() {
            match definition {
                templiqx_contracts::ComponentDefinition::Typed(component) => {
                    let content = std::mem::take(&mut component.content);
                    component.content = self.expand_includes(package, content, &mut Vec::new())?;
                }
                templiqx_contracts::ComponentDefinition::Legacy(nodes) => {
                    let content = std::mem::take(nodes);
                    *nodes = self.expand_includes(package, content, &mut Vec::new())?;
                }
            }
        }
        Ok(parsed)
    }

    /// Recursively replace `Include` nodes with the parsed content of the
    /// referenced partial. `stack` holds `pkg::path` keys already being expanded
    /// so cycles fail with a stable diagnostic. Path confinement is enforced by
    /// the store (traversal rejected); missing/invalid partials fail closed.
    fn expand_includes(
        &self,
        package: &str,
        nodes: Vec<templiqx_contracts::Node>,
        stack: &mut Vec<String>,
    ) -> Result<Vec<templiqx_contracts::Node>, Vec<Diagnostic>> {
        use templiqx_contracts::Node;
        let mut out = Vec::with_capacity(nodes.len());
        for node in nodes {
            match node {
                Node::Include {
                    path,
                    from_dependency,
                } => {
                    let target = from_dependency.as_deref().unwrap_or(package);
                    let key = format!("{target}::{path}");
                    if stack.contains(&key) {
                        return Err(vec![Diagnostic::error(
                            "TQX_INCLUDE_CYCLE",
                            format!("include cycle detected at '{key}'"),
                            "",
                        )]);
                    }
                    let bytes = self.store.artifact_bytes(target, &path).map_err(|error| {
                        vec![Diagnostic::error(
                            "TQX_INCLUDE_UNRESOLVED",
                            format!("cannot resolve include '{key}': {error}"),
                            "",
                        )]
                    })?;
                    let partial: Vec<Node> =
                        serde_yaml_ng::from_slice(&bytes).map_err(|error| {
                            vec![Diagnostic::error(
                                "TQX_INCLUDE_INVALID",
                                format!("include '{key}' is not a valid node list: {error}"),
                                "",
                            )]
                        })?;
                    stack.push(key);
                    let expanded = self.expand_includes(target, partial, stack)?;
                    stack.pop();
                    out.extend(expanded);
                }
                Node::When {
                    condition,
                    then,
                    otherwise,
                } => out.push(Node::When {
                    condition,
                    then: self.expand_includes(package, then, stack)?,
                    otherwise: self.expand_includes(package, otherwise, stack)?,
                }),
                Node::ForEach {
                    collection,
                    item,
                    body,
                    separator,
                } => out.push(Node::ForEach {
                    collection,
                    item,
                    body: self.expand_includes(package, body, stack)?,
                    separator,
                }),
                other => out.push(other),
            }
        }
        Ok(out)
    }

    pub fn catalog(&self) -> OperationEnvelope<Vec<String>> {
        catalog()
    }

    pub fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>> {
        port_result("discover_packages", self.store.discover())
    }

    pub fn create_package(
        &self,
        request: &CreatePackageRequest,
    ) -> OperationEnvelope<PackageManifest> {
        match self.store.create_package(&request.name, &request.version) {
            Ok(manifest) => with_hash(
                OperationEnvelope::new("create_package", Some(manifest.clone()), vec![]),
                "package",
                &manifest,
            ),
            Err(e) => port_failure("create_package", e),
        }
    }

    pub fn update_package(
        &self,
        request: &UpdatePackageRequest,
    ) -> OperationEnvelope<PackageManifest> {
        if request.version.is_none() && request.description.is_none() {
            return OperationEnvelope::new(
                "update_package",
                None,
                vec![Diagnostic::error(
                    "TQX_PACKAGE_UPDATE_EMPTY",
                    "version or description must be provided",
                    "/",
                )],
            );
        }
        if request
            .version
            .as_deref()
            .is_some_and(|version| semver::Version::parse(version).is_err())
        {
            return OperationEnvelope::new(
                "update_package",
                None,
                vec![Diagnostic::error(
                    "TQX_MANIFEST_VERSION_INVALID",
                    "manifest version must be semantic versioning",
                    "/version",
                )],
            );
        }
        match self.store.update_package(
            &request.package,
            request.version.as_deref(),
            request.description.as_deref(),
            &request.expected_fingerprint,
        ) {
            Ok(manifest) => with_hash(
                OperationEnvelope::new("update_package", Some(manifest.clone()), vec![]),
                "package",
                &manifest,
            ),
            Err(error) => port_failure("update_package", error),
        }
    }

    pub fn delete_package(
        &self,
        request: &DeletePackageRequest,
    ) -> OperationEnvelope<PackageManifest> {
        let manifest = match self.store.manifest(&request.package) {
            Ok(manifest) => manifest,
            Err(error) => return port_failure("delete_package", error),
        };
        match self
            .store
            .delete_package(&request.package, &request.expected_fingerprint)
        {
            Ok(hash) => OperationEnvelope::new("delete_package", Some(manifest), vec![])
                .fingerprint("package", hash),
            Err(error) => port_failure("delete_package", error),
        }
    }

    pub fn export_package_identity(&self, package: &str) -> OperationEnvelope<PackageIdentity> {
        match self.canonical_package_identity(package) {
            Ok(identity) => {
                let manifest_hash = self
                    .store
                    .manifest(package)
                    .ok()
                    .and_then(|manifest| fingerprint(&manifest).ok())
                    .unwrap_or_default();
                with_hash(
                    OperationEnvelope::new(
                        "export_package_identity",
                        Some(identity.clone()),
                        vec![],
                    )
                    .fingerprint("manifest", manifest_hash),
                    "package_identity",
                    &identity,
                )
            }
            Err(error) => port_failure("export_package_identity", error),
        }
    }

    pub fn sign_package(&self, request: &SignPackageRequest) -> OperationEnvelope<PackageManifest> {
        let Some(key) = package_signing_key() else {
            return OperationEnvelope::new(
                "sign_package",
                None,
                vec![Diagnostic::error(
                    "TQX_PACKAGE_SIGNING_KEY_MISSING",
                    "TEMPLIQX_PACKAGE_SIGNING_KEY is required for local dev/CI signing",
                    "/signatures",
                )],
            );
        };
        let identity = match self.canonical_package_identity(&request.package) {
            Ok(identity) => identity,
            Err(error) => return port_failure("sign_package", error),
        };
        let signature = match sign_package_identity(&identity, key.as_bytes(), &request.key_id) {
            Ok(signature) => signature,
            Err(error) => {
                return OperationEnvelope::new(
                    "sign_package",
                    None,
                    vec![Diagnostic::error(
                        "TQX_PACKAGE_SIGNATURE_INVALID",
                        error.to_string(),
                        "/signatures",
                    )],
                );
            }
        };
        let identity_fingerprint = match fingerprint(&identity) {
            Ok(value) => value,
            Err(error) => {
                return OperationEnvelope::new(
                    "sign_package",
                    None,
                    vec![Diagnostic::error(
                        "TQX_PACKAGE_SIGNATURE_INVALID",
                        error.to_string(),
                        "/signatures",
                    )],
                );
            }
        };
        match self.store.attach_package_signature(
            &request.package,
            signature,
            &request.expected_fingerprint,
            &identity_fingerprint,
        ) {
            Ok(manifest) => with_hash(
                OperationEnvelope::new("sign_package", Some(manifest.clone()), vec![]),
                "package",
                &manifest,
            ),
            Err(error) => port_failure("sign_package", error),
        }
    }

    pub fn verify_package_trust(
        &self,
        request: &VerifyPackageTrustRequest,
    ) -> OperationEnvelope<PackageTrustReport> {
        let identity = match self.canonical_package_identity(&request.package) {
            Ok(identity) => identity,
            Err(error) => return port_failure("verify_package_trust", error),
        };
        let manifest = match self.store.manifest(&request.package) {
            Ok(manifest) => manifest,
            Err(error) => return port_failure("verify_package_trust", error),
        };
        let mut diagnostics = Vec::new();
        let verified_key_ids = verify_package_signatures_with_mode(
            &identity,
            &manifest.signatures,
            package_signing_key(),
            request.strict,
            &mut diagnostics,
        );
        let identity_fingerprint = fingerprint(&identity).unwrap_or_default();
        OperationEnvelope::new(
            "verify_package_trust",
            Some(PackageTrustReport {
                identity_fingerprint: identity_fingerprint.clone(),
                strict: request.strict,
                verified_key_ids,
            }),
            diagnostics,
        )
        .fingerprint("package_identity", identity_fingerprint)
    }

    fn canonical_package_identity(&self, package: &str) -> Result<PackageIdentity, PortError> {
        self.store.package_identity(package)
    }

    pub fn delete_contract(
        &self,
        request: &DeleteContractRequest,
    ) -> OperationEnvelope<ContractSummary> {
        let parsed = match self.load_contract(&request.package, &request.contract) {
            Ok(v) => v,
            Err(diagnostics) => {
                return OperationEnvelope::new("delete_contract", None, diagnostics);
            }
        };
        match self.store.delete_contract(
            &request.package,
            &request.contract,
            &request.expected_fingerprint,
        ) {
            Ok(hash) => OperationEnvelope::new(
                "delete_contract",
                Some(ContractSummary {
                    package: request.package.clone(),
                    id: parsed.id,
                    version: parsed.version,
                    fingerprint: hash.clone(),
                }),
                vec![],
            )
            .fingerprint("contract", hash),
            Err(e) => port_failure("delete_contract", e),
        }
    }

    pub fn inspect_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Contract> {
        match self.load_contract(package, contract) {
            Ok(value) => with_hash(
                OperationEnvelope::new("inspect_contract", Some(value.clone()), vec![]),
                "contract",
                &value,
            ),
            Err(diagnostics) => OperationEnvelope::new("inspect_contract", None, diagnostics),
        }
    }

    pub fn put_contract(
        &self,
        package: &str,
        contract: &str,
        source: &str,
        expected_fingerprint: Option<&str>,
    ) -> OperationEnvelope<ContractSummary> {
        let parsed = match templiqx_core::parse_contract(source, Some(contract)) {
            Ok(v) => v,
            Err(d) => return OperationEnvelope::new("put_contract", None, d),
        };
        let diagnostics = templiqx_core::validate_contract(&parsed);
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return OperationEnvelope::new("put_contract", None, diagnostics);
        }
        match self
            .store
            .put_contract(package, contract, source, expected_fingerprint)
        {
            Ok(hash) => OperationEnvelope::new(
                "put_contract",
                Some(ContractSummary {
                    package: package.into(),
                    id: parsed.id,
                    version: parsed.version,
                    fingerprint: hash.clone(),
                }),
                diagnostics,
            )
            .fingerprint("contract", hash),
            Err(e) => port_failure("put_contract", e),
        }
    }

    pub fn validate_contract(
        &self,
        package: &str,
        contract: &str,
    ) -> OperationEnvelope<ContractSummary> {
        let value = match self.load_contract(package, contract) {
            Ok(v) => v,
            Err(diagnostics) => {
                return OperationEnvelope::new("validate_contract", None, diagnostics);
            }
        };
        let mut diagnostics = templiqx_core::validate_contract(&value);
        address(&mut diagnostics, package, contract);
        let hash = match fingerprint(&value) {
            Ok(v) => v,
            Err(e) => return serialization_failure("validate_contract", e),
        };
        OperationEnvelope::new(
            "validate_contract",
            Some(ContractSummary {
                package: package.into(),
                id: value.id,
                version: value.version,
                fingerprint: hash.clone(),
            }),
            diagnostics,
        )
        .fingerprint("contract", hash)
    }

    pub fn validate_package(&self, package: &str) -> OperationEnvelope<Vec<ContractSummary>> {
        let manifest = match self.store.manifest(package) {
            Ok(v) => v,
            Err(e) => return port_failure("validate_package", e),
        };
        let mut diagnostics = Vec::new();
        let mut summaries = Vec::new();
        if manifest.api_version != templiqx_contracts::API_VERSION {
            diagnostics.push(Diagnostic::error(
                "TQX_MANIFEST_API_VERSION",
                "unsupported manifest api_version",
                "/api_version",
            ));
        }
        if manifest.package != package {
            diagnostics.push(Diagnostic::error(
                "TQX_MANIFEST_PACKAGE",
                "manifest package does not match directory",
                "/package",
            ));
        }
        if semver::Version::parse(&manifest.version).is_err() {
            diagnostics.push(Diagnostic::error(
                "TQX_MANIFEST_VERSION_INVALID",
                "manifest version must be semantic versioning",
                "/version",
            ));
        }
        let mut inventory = Vec::new();
        inventory.extend(
            manifest
                .contracts
                .iter()
                .map(|id| (format!("contracts/{id}.yaml"), format!("/contracts/{id}"))),
        );
        for (section, entries) in [
            ("components", &manifest.components),
            ("evals", &manifest.evals),
            ("migrations", &manifest.migrations),
            ("templates", &manifest.templates),
        ] {
            inventory.extend(
                entries
                    .iter()
                    .map(|path| (path.clone(), format!("/{section}/{path}"))),
            );
        }
        inventory.sort_by(|left, right| left.0.cmp(&right.0));
        let mut artifact_hashes = std::collections::BTreeMap::new();
        let mut seen_artifacts = std::collections::BTreeSet::new();
        for (path, pointer) in inventory {
            if !seen_artifacts.insert(path.clone()) {
                diagnostics.push(Diagnostic::error(
                    "TQX_INVENTORY_DUPLICATE",
                    format!("artifact '{path}' is listed more than once"),
                    pointer,
                ));
                continue;
            }
            match self.store.artifact_bytes(package, &path) {
                Ok(bytes) => {
                    artifact_hashes.insert(path, fingerprint_bytes(&bytes));
                }
                Err(error) => {
                    let mut diagnostic = port_diagnostic(error);
                    diagnostic.file = Some(format!("{package}/{path}"));
                    diagnostic.json_pointer = Some(pointer);
                    diagnostics.push(diagnostic);
                }
            }
        }
        for id in &manifest.contracts {
            match self.load_contract(package, id) {
                Ok(contract) => {
                    if contract.id != *id {
                        diagnostics.push(Diagnostic::error(
                            "TQX_CONTRACT_INVENTORY_ID",
                            format!(
                                "contract id '{}' does not match manifest inventory id '{id}'",
                                contract.id
                            ),
                            format!("/contracts/{id}"),
                        ));
                    }
                    let mut found = templiqx_core::validate_contract(&contract);
                    address(&mut found, package, id);
                    diagnostics.extend(found);
                    match fingerprint(&contract) {
                        Ok(hash) => summaries.push(ContractSummary {
                            package: package.into(),
                            id: contract.id,
                            version: contract.version,
                            fingerprint: hash,
                        }),
                        Err(e) => {
                            diagnostics.push(Diagnostic::error("TQX_SERIALIZE", e.to_string(), ""))
                        }
                    }
                }
                Err(found) => diagnostics.extend(found),
            }
        }
        // U3 (plan 001): dependency declarations verified against templiqx.lock.
        // Content-addressed, no registry, no network fetch.
        let lock: Option<templiqx_contracts::PackageLock> =
            match self.store.artifact_bytes(package, "templiqx.lock") {
                Ok(bytes) => {
                    artifact_hashes.insert("templiqx.lock".into(), fingerprint_bytes(&bytes));
                    match serde_yaml_ng::from_slice(&bytes) {
                        Ok(parsed) => Some(parsed),
                        Err(error) => {
                            diagnostics.push(Diagnostic::error(
                                "TQX_LOCK_INVALID",
                                format!("templiqx.lock is not valid: {error}"),
                                "/templiqx.lock",
                            ));
                            None
                        }
                    }
                }
                Err(_) => None,
            };
        if !manifest.dependencies.is_empty() {
            match &lock {
                None => diagnostics.push(Diagnostic::error(
                    "TQX_LOCK_MISSING",
                    "package declares dependencies but has no templiqx.lock",
                    "/dependencies",
                )),
                Some(lock) => {
                    for (name, expected_fingerprint) in &manifest.dependencies {
                        match lock.dependencies.get(name) {
                            Some(locked) if &locked.fingerprint == expected_fingerprint => {
                                if self.store.manifest(name).is_err() {
                                    diagnostics.push(Diagnostic::error(
                                        "TQX_DEPENDENCY_ROOT_MISSING",
                                        format!("dependency '{name}' root not found in workspace"),
                                        format!("/dependencies/{name}"),
                                    ));
                                }
                            }
                            Some(_) => diagnostics.push(Diagnostic::error(
                                "TQX_LOCK_DRIFT",
                                format!("lock fingerprint for '{name}' differs from manifest"),
                                format!("/dependencies/{name}"),
                            )),
                            None => diagnostics.push(Diagnostic::error(
                                "TQX_LOCK_DRIFT",
                                format!("dependency '{name}' is not pinned in templiqx.lock"),
                                format!("/dependencies/{name}"),
                            )),
                        }
                    }
                }
            }
        }
        if let Some(lock) = &lock {
            for name in lock.dependencies.keys() {
                if !manifest.dependencies.contains_key(name) {
                    diagnostics.push(Diagnostic::error(
                        "TQX_LOCK_DRIFT",
                        format!("templiqx.lock pins '{name}' which the manifest does not declare"),
                        "/templiqx.lock",
                    ));
                }
            }
        }

        let signatures = manifest.signatures.clone();
        let mut normalized_manifest = manifest;
        normalized_manifest.signatures.clear();
        normalized_manifest.contracts.sort();
        normalized_manifest.components.sort();
        normalized_manifest.evals.sort();
        normalized_manifest.migrations.sort();
        normalized_manifest.templates.sort();
        let package_identity =
            serde_json::json!({"manifest": normalized_manifest, "artifacts": artifact_hashes});
        verify_package_signatures(
            &package_identity,
            &signatures,
            package_signing_key(),
            &mut diagnostics,
        );
        let envelope = OperationEnvelope::new("validate_package", Some(summaries), diagnostics);
        if envelope.ok {
            with_hash(envelope, "package", &package_identity)
        } else {
            envelope
        }
    }

    pub fn compile_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
    ) -> OperationEnvelope<templiqx_contracts::CompiledInteraction> {
        let value = match self.load_contract(package, contract) {
            Ok(v) => v,
            Err(diagnostics) => {
                return OperationEnvelope::new("compile_contract", None, diagnostics);
            }
        };
        match templiqx_core::compile(&value, request, capabilities) {
            Ok(compiled) => with_hash(
                OperationEnvelope::new("compile_contract", Some(compiled.clone()), vec![]),
                "compiled_interaction",
                &compiled,
            ),
            Err(mut diagnostics) => {
                address(&mut diagnostics, package, contract);
                OperationEnvelope::new("compile_contract", None, diagnostics)
            }
        }
    }

    pub fn render_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
    ) -> OperationEnvelope<Vec<templiqx_contracts::CompiledMessage>> {
        let compiled = self.compile_contract(package, contract, request, capabilities);
        if !compiled.ok {
            return OperationEnvelope::new("render_contract", None, compiled.diagnostics);
        }
        let messages = compiled
            .result
            .expect("successful compilation has result")
            .messages;
        with_hash(
            OperationEnvelope::new("render_contract", Some(messages.clone()), vec![]),
            "rendered_messages",
            &messages,
        )
    }

    pub fn execute_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
        fixture_output: Option<Value>,
        stream: bool,
    ) -> OperationEnvelope<templiqx_contracts::ExecutionReceipt> {
        let compiled = self.compile_contract(package, contract, request, capabilities);
        if !compiled.ok {
            return OperationEnvelope::new("execute_contract", None, compiled.diagnostics);
        }
        let interaction = compiled.result.expect("successful compilation has result");
        let runtime = self.runtime.descriptor();
        for required in &interaction.required_capabilities {
            if !runtime.capabilities.contains(required) {
                return OperationEnvelope::new(
                    "execute_contract",
                    None,
                    vec![Diagnostic::error(
                        "TQX_RUNTIME_CAPABILITY_UNSUPPORTED",
                        format!("runtime lacks '{required}'"),
                        "/capabilities",
                    )],
                );
            }
        }
        let execution_request = ExecutionRequest {
            interaction,
            fixture_output,
        };
        let mut stream_events = Vec::new();
        let executed = if stream {
            self.runtime
                .execute_streaming(&execution_request, &mut |event| {
                    stream_events.push(event);
                })
        } else {
            self.runtime.execute(&execution_request)
        };
        match executed {
            Ok(receipt) => {
                let mut diagnostics = vec![];
                if !receipt.output_schema_valid {
                    diagnostics.push(Diagnostic::error(
                        "TQX_OUTPUT_SCHEMA",
                        "runtime output does not satisfy the contract schema",
                        "/output",
                    ));
                }
                let mut envelope =
                    OperationEnvelope::new("execute_contract", Some(receipt.clone()), diagnostics)
                        .fingerprint("request", receipt.request_fingerprint.clone())
                        .fingerprint("output", receipt.output_fingerprint.clone());
                if stream {
                    envelope = envelope.stream_events(stream_events);
                }
                envelope
            }
            Err(e) => port_failure("execute_contract", e),
        }
    }

    pub fn test_package(
        &self,
        package: &str,
        capabilities: &[String],
    ) -> OperationEnvelope<TestReport> {
        let manifest = match self.store.manifest(package) {
            Ok(v) => v,
            Err(e) => return port_failure("test_package", e),
        };
        let mut cases = Vec::new();
        for contract_id in &manifest.contracts {
            let contract = match self.load_contract(package, contract_id) {
                Ok(v) => v,
                Err(diagnostics) => {
                    cases.push(TestCaseResult {
                        contract_id: contract_id.clone(),
                        fixture_id: String::new(),
                        passed: false,
                        diagnostics,
                        artifact_fingerprint: None,
                    });
                    continue;
                }
            };
            for fixture in &contract.evals {
                let envelope = self.execute_contract(
                    package,
                    contract_id,
                    &RenderRequest {
                        inputs: fixture.inputs.clone(),
                        context: fixture.context.clone(),
                    },
                    capabilities,
                    Some(fixture.fake_output.clone()),
                    false,
                );
                cases.push(TestCaseResult {
                    contract_id: contract_id.clone(),
                    fixture_id: fixture.id.clone(),
                    passed: envelope.ok,
                    diagnostics: envelope.diagnostics,
                    artifact_fingerprint: envelope.fingerprints.get("output").cloned(),
                });
            }
        }
        let passed = cases.iter().filter(|c| c.passed).count();
        let failed = cases.len() - passed;
        let diagnostics = if failed == 0 {
            vec![]
        } else {
            vec![Diagnostic::error(
                "TQX_TEST_FAILED",
                format!("{failed} fixture(s) failed"),
                "/cases",
            )]
        };
        OperationEnvelope::new(
            "test_package",
            Some(TestReport {
                package: package.into(),
                passed,
                failed,
                cases,
            }),
            diagnostics,
        )
    }

    pub fn list_evals(&self, package: &str) -> OperationEnvelope<Vec<EvalCase>> {
        let manifest = match self.store.manifest(package) {
            Ok(v) => v,
            Err(e) => return port_failure("list_evals", e),
        };
        let mut cases = Vec::new();
        let mut diagnostics = Vec::new();
        for contract_id in &manifest.contracts {
            match self.load_contract(package, contract_id) {
                Ok(contract) => cases.extend(contract.evals.iter().map(|fixture| EvalCase {
                    contract_id: contract_id.clone(),
                    fixture_id: fixture.id.clone(),
                })),
                Err(found) => diagnostics.extend(found),
            }
        }
        OperationEnvelope::new("list_evals", Some(cases), diagnostics)
    }

    pub fn run_eval(
        &self,
        package: &str,
        contract: &str,
        fixture_id: &str,
        capabilities: &[String],
    ) -> OperationEnvelope<TestCaseResult> {
        let parsed = match self.load_contract(package, contract) {
            Ok(v) => v,
            Err(diagnostics) => return OperationEnvelope::new("run_eval", None, diagnostics),
        };
        let Some(fixture) = parsed.evals.iter().find(|f| f.id == fixture_id) else {
            return OperationEnvelope::new(
                "run_eval",
                None,
                vec![Diagnostic::error(
                    "TQX_NOT_FOUND",
                    format!("eval fixture '{fixture_id}' not found for contract '{contract}'"),
                    "/evals",
                )],
            );
        };
        let envelope = self.execute_contract(
            package,
            contract,
            &RenderRequest {
                inputs: fixture.inputs.clone(),
                context: fixture.context.clone(),
            },
            capabilities,
            Some(fixture.fake_output.clone()),
            false,
        );
        let case = TestCaseResult {
            contract_id: contract.into(),
            fixture_id: fixture_id.into(),
            passed: envelope.ok,
            diagnostics: envelope.diagnostics,
            artifact_fingerprint: envelope.fingerprints.get("output").cloned(),
        };
        let diagnostics = if case.passed {
            vec![]
        } else {
            vec![Diagnostic::error(
                "TQX_TEST_FAILED",
                format!("fixture '{fixture_id}' failed"),
                "/case",
            )]
        };
        OperationEnvelope::new("run_eval", Some(case), diagnostics)
    }

    pub fn diff_contract(
        &self,
        left_package: &str,
        left: &str,
        right_package: &str,
        right: &str,
    ) -> OperationEnvelope<ContractDiff> {
        let a = match self.load_contract(left_package, left) {
            Ok(v) => v,
            Err(diagnostics) => return OperationEnvelope::new("diff_contract", None, diagnostics),
        };
        let b = match self.load_contract(right_package, right) {
            Ok(v) => v,
            Err(diagnostics) => return OperationEnvelope::new("diff_contract", None, diagnostics),
        };
        let ah = fingerprint(&a).unwrap_or_default();
        let bh = fingerprint(&b).unwrap_or_default();
        let mut changes = Vec::new();
        if a.id != b.id {
            changes.push("id".into());
        }
        if a.version != b.version {
            changes.push("version".into());
        }
        if a.inputs != b.inputs {
            changes.push("inputs".into());
        }
        if a.context != b.context {
            changes.push("context".into());
        }
        if a.messages != b.messages {
            changes.push("messages".into());
        }
        if a.output_schema != b.output_schema {
            changes.push("output_schema".into());
        }
        if a.capabilities != b.capabilities {
            changes.push("capabilities".into());
        }
        OperationEnvelope::new(
            "diff_contract",
            Some(ContractDiff {
                equal: ah == bh,
                left_fingerprint: ah,
                right_fingerprint: bh,
                changes,
            }),
            vec![],
        )
    }

    pub fn explain_contract(
        &self,
        package: &str,
        contract: &str,
    ) -> OperationEnvelope<Explanation> {
        let c = match self.load_contract(package, contract) {
            Ok(v) => v,
            Err(diagnostics) => {
                return OperationEnvelope::new("explain_contract", None, diagnostics);
            }
        };
        let defined: std::collections::BTreeSet<String> = c.components.keys().cloned().collect();
        let mut referenced = std::collections::BTreeSet::new();
        for message in &c.messages {
            collect_component_refs(&message.content, &mut referenced);
        }
        for definition in c.components.values() {
            let body = match definition {
                templiqx_contracts::ComponentDefinition::Typed(t) => &t.content,
                templiqx_contracts::ComponentDefinition::Legacy(nodes) => nodes,
            };
            collect_component_refs(body, &mut referenced);
        }
        let unresolved_references: Vec<String> = referenced.difference(&defined).cloned().collect();

        let mut fix_hints = Vec::new();
        for name in &unresolved_references {
            fix_hints.push(format!(
                "define component '{name}' under `components:` or remove its reference (TQX_COMPONENT_UNDEFINED)"
            ));
        }
        if c.messages.is_empty() {
            fix_hints
                .push("add at least one message under `messages:` (TQX_MESSAGES_EMPTY)".to_owned());
        }

        OperationEnvelope::new(
            "explain_contract",
            Some(Explanation {
                contract_id: c.id,
                summary: c.description,
                inputs: c.inputs.keys().cloned().collect(),
                context: c.context.keys().cloned().collect(),
                capabilities: c.capabilities,
                component_count: c.components.len(),
                components: defined.into_iter().collect(),
                unresolved_references,
                fix_hints,
            }),
            vec![],
        )
    }

    pub fn migrate_legacy(
        &self,
        request: &MigrateLegacyRequest,
    ) -> OperationEnvelope<MigrationResult> {
        let source = match self
            .store
            .resolve_artifact_path(&request.package, &request.source)
        {
            Ok(path) => path,
            Err(error) => return port_failure("migrate_legacy", error),
        };
        let migrated = match self.legacy.migrate(&AdapterLegacyImportRequest {
            dialect: request.dialect.clone(),
            source,
            aliases: request.aliases.clone(),
        }) {
            Ok(result) => result,
            Err(error) => return port_failure("migrate_legacy", error),
        };
        let canonical_template = match migrated.canonical_template {
            Some(path) => match self.store.relative_artifact_path(&request.package, &path) {
                Ok(path) => Some(path),
                Err(error) => return port_failure("migrate_legacy", error),
            },
            None => None,
        };
        OperationEnvelope::new(
            "migrate_legacy",
            Some(MigrationResult {
                report: migrated.report,
                canonical_template,
            }),
            vec![],
        )
    }

    pub fn render_document(
        &self,
        request: &RenderDocumentRequest,
    ) -> OperationEnvelope<RenderDocumentResult> {
        let template = match self
            .store
            .resolve_artifact_path(&request.package, &request.template)
        {
            Ok(path) => path,
            Err(error) => return port_failure("render_document", error),
        };
        let output_lease = match self.workspace.lease_output_path(
            &request.package,
            &request.output,
            request.workspace.as_deref(),
        ) {
            Ok(lease) => lease,
            Err(error) => return port_failure("render_document", error),
        };
        let rendered = match self
            .documents
            .render_document(&AdapterDocumentRenderRequest {
                template,
                data: request.data.clone(),
                output: output_lease.path().to_path_buf(),
            }) {
            Ok(result) => result,
            Err(error) => return port_failure("render_document", error),
        };
        let artifact = match self.workspace.relative_artifact_path(
            &request.package,
            &rendered.artifact,
            request.workspace.as_deref(),
        ) {
            Ok(path) => path,
            Err(error) => return port_failure("render_document", error),
        };
        OperationEnvelope::new(
            "render_document",
            Some(RenderDocumentResult {
                artifact,
                report: rendered.report,
            }),
            vec![],
        )
    }

    pub fn list_workspace_artifacts(
        &self,
        request: &ListWorkspaceArtifactsRequest,
    ) -> OperationEnvelope<Vec<WorkspaceArtifact>> {
        let entries = match self.workspace.list_artifacts(
            &request.package,
            request.workspace.as_deref(),
            request.prefix.as_deref(),
        ) {
            Ok(entries) => entries,
            Err(error) => return port_failure("list_workspace_artifacts", error),
        };
        let mut artifacts = Vec::with_capacity(entries.len());
        for (path, size) in entries {
            match self.workspace.relative_artifact_path(
                &request.package,
                &path,
                request.workspace.as_deref(),
            ) {
                Ok(portable) => artifacts.push(WorkspaceArtifact {
                    path: portable,
                    size,
                }),
                Err(error) => return port_failure("list_workspace_artifacts", error),
            }
        }
        artifacts.sort_by(|a, b| a.path.cmp(&b.path));
        OperationEnvelope::new("list_workspace_artifacts", Some(artifacts), vec![])
    }

    pub fn read_artifact(
        &self,
        request: &ReadArtifactRequest,
    ) -> OperationEnvelope<ArtifactContent> {
        let bytes = match self.workspace.read_artifact(
            &request.package,
            &request.path,
            request.workspace.as_deref(),
        ) {
            Ok(bytes) => bytes,
            Err(error) => return port_failure("read_artifact", error),
        };
        let hash = fingerprint_bytes(&bytes);
        let (content_encoding, content) = encode_artifact_content(&request.path, &bytes);
        OperationEnvelope::new(
            "read_artifact",
            Some(ArtifactContent {
                path: request.path.clone(),
                content_type: content_type_for(&request.path),
                content_encoding,
                content,
            }),
            vec![],
        )
        .fingerprint("artifact", hash)
    }

    pub fn delete_workspace_artifact(
        &self,
        request: &DeleteWorkspaceArtifactRequest,
    ) -> OperationEnvelope<WorkspaceArtifact> {
        let size = match self.workspace.read_artifact(
            &request.package,
            &request.path,
            request.workspace.as_deref(),
        ) {
            Ok(bytes) => bytes.len() as u64,
            Err(error) => return port_failure("delete_workspace_artifact", error),
        };
        match self.workspace.delete_artifact(
            &request.package,
            &request.path,
            request.workspace.as_deref(),
            &request.expected_fingerprint,
        ) {
            Ok(hash) => OperationEnvelope::new(
                "delete_workspace_artifact",
                Some(WorkspaceArtifact {
                    path: request.path.clone(),
                    size,
                }),
                vec![],
            )
            .fingerprint("artifact", hash),
            Err(error) => port_failure("delete_workspace_artifact", error),
        }
    }
}

pub const PACKAGE_SIGNATURE_ALGORITHM: &str = "sha256-keyed";

pub fn package_identity_bytes(identity: &impl Serialize) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(identity)
}

pub fn sign_package_identity(
    identity: &impl Serialize,
    key: &[u8],
    key_id: &str,
) -> Result<PackageSignature, serde_json::Error> {
    use sha2::{Digest, Sha256};
    let payload = package_identity_bytes(identity)?;
    let mut hasher = Sha256::new();
    hasher.update(b"templiqx-package-signing-v1\0");
    hasher.update(key);
    hasher.update(b"\0key-id\0");
    hasher.update(key_id.as_bytes());
    hasher.update(b"\0algorithm\0");
    hasher.update(PACKAGE_SIGNATURE_ALGORITHM.as_bytes());
    hasher.update(b"\0payload\0");
    hasher.update(&payload);
    Ok(PackageSignature {
        key_id: key_id.into(),
        algorithm: PACKAGE_SIGNATURE_ALGORITHM.into(),
        value: hex::encode(hasher.finalize()),
    })
}

fn package_signing_key() -> Option<String> {
    std::env::var("TEMPLIQX_PACKAGE_SIGNING_KEY")
        .ok()
        .filter(|value| !value.is_empty())
}

pub fn verify_package_signatures(
    identity: &impl Serialize,
    signatures: &[PackageSignature],
    signing_key: Option<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let strict = std::env::var("TEMPLIQX_PACKAGE_STRICT").ok().as_deref() == Some("1");
    let _ =
        verify_package_signatures_with_mode(identity, signatures, signing_key, strict, diagnostics);
}

pub fn verify_package_signatures_with_mode(
    identity: &impl Serialize,
    signatures: &[PackageSignature],
    signing_key: Option<String>,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<String> {
    if signatures.is_empty() {
        if strict {
            diagnostics.push(Diagnostic {
                code: "TQX_PACKAGE_UNSIGNED".into(),
                severity: Severity::Error,
                message: "package has no manifest signatures".into(),
                file: None,
                json_pointer: Some("/signatures".into()),
                span: None,
                help: Some(
                    "set TEMPLIQX_PACKAGE_SIGNING_KEY and add signatures for strict publication"
                        .into(),
                ),
            });
        }
        return Vec::new();
    }
    let mut seen = std::collections::BTreeSet::new();
    for signature in signatures {
        if signature.algorithm != PACKAGE_SIGNATURE_ALGORITHM {
            diagnostics.push(Diagnostic::error(
                "TQX_PACKAGE_SIGNATURE_ALGORITHM_UNSUPPORTED",
                format!(
                    "unsupported package signature algorithm '{}'",
                    signature.algorithm
                ),
                "/signatures",
            ));
        }
        if !seen.insert((&signature.key_id, &signature.algorithm)) {
            diagnostics.push(Diagnostic::error(
                "TQX_PACKAGE_SIGNATURE_DUPLICATE",
                format!(
                    "duplicate package signature for key '{}' and algorithm '{}'",
                    signature.key_id, signature.algorithm
                ),
                "/signatures",
            ));
        }
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Vec::new();
    }
    let Some(key) = signing_key else {
        diagnostics.push(Diagnostic::error(
            "TQX_PACKAGE_SIGNATURE_UNVERIFIED",
            "package signatures present but TEMPLIQX_PACKAGE_SIGNING_KEY is unset",
            "/signatures",
        ));
        return Vec::new();
    };
    let mut verified = Vec::with_capacity(signatures.len());
    for signature in signatures {
        let expected = match sign_package_identity(identity, key.as_bytes(), &signature.key_id) {
            Ok(expected) => expected,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "TQX_PACKAGE_SIGNATURE_INVALID",
                    error.to_string(),
                    "/signatures",
                ));
                return Vec::new();
            }
        };
        if signature.value != expected.value {
            diagnostics.push(Diagnostic::error(
                "TQX_PACKAGE_SIGNATURE_INVALID",
                format!(
                    "signature for key '{}' does not match package identity",
                    signature.key_id
                ),
                "/signatures",
            ));
        } else {
            verified.push(signature.key_id.clone());
        }
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        Vec::new()
    } else {
        verified
    }
}

/// U6: walk contract content, recording every component name referenced by a
/// `component` node (recursing through `when`/`for_each` bodies and nested
/// component invocations). Used by `explain_contract` to surface unresolved refs.
fn collect_component_refs(
    nodes: &[templiqx_contracts::Node],
    out: &mut std::collections::BTreeSet<String>,
) {
    use templiqx_contracts::Node;
    for node in nodes {
        match node {
            Node::Component { name, .. } => {
                out.insert(name.clone());
            }
            Node::When {
                then, otherwise, ..
            } => {
                collect_component_refs(then, out);
                collect_component_refs(otherwise, out);
            }
            Node::ForEach { body, .. } => collect_component_refs(body, out),
            Node::Text { .. } | Node::Interpolate { .. } | Node::Include { .. } => {}
        }
    }
}

fn with_hash<T: Serialize>(
    envelope: OperationEnvelope<T>,
    name: &str,
    value: &impl Serialize,
) -> OperationEnvelope<T> {
    match fingerprint(value) {
        Ok(hash) => envelope.fingerprint(name, hash),
        Err(_) => envelope,
    }
}
fn port_result<T>(operation: &str, result: Result<T, PortError>) -> OperationEnvelope<T> {
    match result {
        Ok(v) => OperationEnvelope::new(operation, Some(v), vec![]),
        Err(e) => port_failure(operation, e),
    }
}
fn port_failure<T>(operation: &str, error: PortError) -> OperationEnvelope<T> {
    OperationEnvelope::new(operation, None, vec![port_diagnostic(error)])
}
fn port_diagnostic(error: PortError) -> Diagnostic {
    let code = match &error {
        PortError::NotFound(_) => "TQX_NOT_FOUND",
        PortError::Conflict(_) => "TQX_CAS_CONFLICT",
        PortError::InvalidPath(_) => "TQX_PATH_INVALID",
        PortError::Unsupported(_) => "TQX_UNSUPPORTED",
        PortError::Io(_) => "TQX_IO",
        PortError::InvalidData(_) => "TQX_DATA_INVALID",
        PortError::RuntimeFailure { code, .. } => *code,
    };
    let mut diagnostic = Diagnostic::error(code, error.to_string(), "");
    if let PortError::RuntimeFailure { failure, .. } = &error {
        diagnostic.help = Some(format!(
            "adapter={} version={} scenario={} retry_after_ms={} fingerprint={}",
            failure.adapter_id,
            failure.adapter_version,
            failure.scenario_id.as_deref().unwrap_or("none"),
            failure
                .retry_after_ms
                .map_or_else(|| "none".into(), |value| value.to_string()),
            failure.fingerprint
        ));
    }
    diagnostic
}
fn serialization_failure<T>(operation: &str, error: serde_json::Error) -> OperationEnvelope<T> {
    OperationEnvelope::new(
        operation,
        None,
        vec![Diagnostic::error("TQX_SERIALIZE", error.to_string(), "")],
    )
}

fn address(diagnostics: &mut [Diagnostic], package: &str, contract: &str) {
    let file = format!("{package}/contracts/{contract}.yaml");
    for diagnostic in diagnostics {
        if diagnostic.file.is_none() {
            diagnostic.file = Some(file.clone());
        }
    }
}

fn content_type_for(path: &str) -> String {
    let extension = Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "json" => "application/json",
        "txt" => "text/plain",
        "yaml" | "yml" => "application/yaml",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
    .to_owned()
}

fn encode_artifact_content(path: &str, bytes: &[u8]) -> (ContentEncoding, String) {
    let extension = Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "json" | "txt" | "yaml" | "yml")
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return (ContentEncoding::Utf8, text.to_owned());
    }
    (ContentEncoding::Base64, BASE64.encode(bytes))
}

pub fn catalog() -> OperationEnvelope<Vec<String>> {
    OperationEnvelope::new(
        "catalog",
        Some(CAPABILITY_CATALOG.iter().map(|s| (*s).to_owned()).collect()),
        vec![],
    )
}
