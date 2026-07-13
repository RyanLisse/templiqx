//! MCP stdio adapter over Templiqx's actor-neutral application capabilities.

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{
        GetPromptRequestParams, GetPromptResult, Implementation, ListPromptsResult,
        ListResourcesResult, PaginatedRequestParams, Prompt, PromptArgument, PromptMessage,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents, Role,
        ServerCapabilities, ServerInfo,
    },
    schemars::{self, JsonSchema},
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use templiqx_application::{
    CreatePackageRequest, DeleteContractRequest, DeletePackageRequest,
    DeleteWorkspaceArtifactRequest, ListWorkspaceArtifactsRequest, MigrateLegacyRequest,
    MigrationResult, ReadArtifactRequest, RenderDocumentRequest, RenderDocumentResult,
    SignPackageRequest, TempliqxService, UpdatePackageRequest, VerifyPackageTrustRequest,
};
use templiqx_contracts::{
    ArtifactContent, CompiledInteraction, CompiledMessage, Contract, ContractDiff, ContractSummary,
    ExecutionReceipt, Explanation, OperationEnvelope, PackageIdentity, PackageManifest,
    PackageTrustReport, RenderRequest, StreamEvent, TestCaseResult, TestReport, WorkspaceArtifact,
};
use templiqx_ports::{
    ArtifactWorkspace, DocumentRenderer, LegacyImportAdapter, PackageStore, RuntimeAdapter,
};

/// Stable MCP tool names, exactly matching the application catalog.
pub const TOOL_CATALOG: &[&str] = templiqx_application::CAPABILITY_CATALOG;

pub const RESOURCE_CATALOG_URI: &str = "templiqx://catalog";
pub const RESOURCE_PACKAGES_URI: &str = "templiqx://packages";
pub const RESOURCE_WORKSPACE_URI: &str = "templiqx://workspace";

/// Object-safe routing view of the canonical application service.
pub trait Operations: Send + Sync + 'static {
    fn catalog(&self) -> OperationEnvelope<Vec<String>>;
    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>>;
    fn create_package(&self, request: &CreatePackageInput) -> OperationEnvelope<PackageManifest>;
    fn update_package(&self, request: &UpdatePackageInput) -> OperationEnvelope<PackageManifest>;
    fn delete_package(&self, request: &DeletePackageInput) -> OperationEnvelope<PackageManifest>;
    fn export_package_identity(&self, package: &str) -> OperationEnvelope<PackageIdentity>;
    fn sign_package(&self, request: &SignPackageInput) -> OperationEnvelope<PackageManifest>;
    fn verify_package_trust(
        &self,
        request: &VerifyPackageTrustInput,
    ) -> OperationEnvelope<PackageTrustReport>;
    fn inspect_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Contract>;
    fn put_contract(&self, request: &PutContractInput) -> OperationEnvelope<ContractSummary>;
    fn delete_contract(&self, request: &DeleteContractInput) -> OperationEnvelope<ContractSummary>;
    fn validate_contract(
        &self,
        package: &str,
        contract: &str,
    ) -> OperationEnvelope<ContractSummary>;
    fn validate_package(&self, package: &str) -> OperationEnvelope<Vec<ContractSummary>>;
    fn compile_contract(
        &self,
        request: &InteractionInput,
    ) -> OperationEnvelope<CompiledInteraction>;
    fn render_contract(
        &self,
        request: &InteractionInput,
    ) -> OperationEnvelope<Vec<CompiledMessage>>;
    fn execute_contract(
        &self,
        request: &ExecuteContractInput,
    ) -> OperationEnvelope<ExecutionReceipt>;
    fn migrate_legacy(&self, request: &MigrateLegacyInput) -> OperationEnvelope<MigrationResult>;
    fn render_document(
        &self,
        request: &RenderDocumentInput,
    ) -> OperationEnvelope<RenderDocumentResult>;
    fn list_workspace_artifacts(
        &self,
        request: &ListWorkspaceArtifactsInput,
    ) -> OperationEnvelope<Vec<WorkspaceArtifact>>;
    fn read_artifact(&self, request: &ReadArtifactInput) -> OperationEnvelope<ArtifactContent>;
    fn delete_workspace_artifact(
        &self,
        request: &DeleteWorkspaceArtifactInput,
    ) -> OperationEnvelope<WorkspaceArtifact>;
    fn test_package(&self, package: &str, capabilities: &[String])
    -> OperationEnvelope<TestReport>;
    fn list_evals(&self, package: &str) -> OperationEnvelope<Vec<templiqx_application::EvalCase>>;
    fn run_eval(&self, request: &RunEvalInput) -> OperationEnvelope<TestCaseResult>;
    fn diff_contract(&self, request: &DiffContractInput) -> OperationEnvelope<ContractDiff>;
    fn explain_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Explanation>;
}

impl<S, W, R, L, D> Operations for TempliqxService<S, W, R, L, D>
where
    S: PackageStore + 'static,
    W: ArtifactWorkspace + 'static,
    R: RuntimeAdapter + 'static,
    L: LegacyImportAdapter + 'static,
    D: DocumentRenderer + 'static,
{
    fn catalog(&self) -> OperationEnvelope<Vec<String>> {
        templiqx_application::catalog()
    }
    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>> {
        self.discover_packages()
    }
    fn create_package(&self, r: &CreatePackageInput) -> OperationEnvelope<PackageManifest> {
        self.create_package(&CreatePackageRequest {
            name: r.name.clone(),
            version: r.version.clone(),
        })
    }
    fn update_package(&self, r: &UpdatePackageInput) -> OperationEnvelope<PackageManifest> {
        self.update_package(&UpdatePackageRequest {
            package: r.package.clone(),
            version: r.version.clone(),
            description: r.description.clone(),
            expected_fingerprint: r.expected_fingerprint.clone(),
        })
    }
    fn delete_package(&self, r: &DeletePackageInput) -> OperationEnvelope<PackageManifest> {
        self.delete_package(&DeletePackageRequest {
            package: r.package.clone(),
            expected_fingerprint: r.expected_fingerprint.clone(),
        })
    }
    fn export_package_identity(&self, package: &str) -> OperationEnvelope<PackageIdentity> {
        self.export_package_identity(package)
    }
    fn sign_package(&self, r: &SignPackageInput) -> OperationEnvelope<PackageManifest> {
        self.sign_package(&SignPackageRequest {
            package: r.package.clone(),
            key_id: r.key_id.clone(),
            expected_fingerprint: r.expected_fingerprint.clone(),
        })
    }
    fn verify_package_trust(
        &self,
        r: &VerifyPackageTrustInput,
    ) -> OperationEnvelope<PackageTrustReport> {
        self.verify_package_trust(&VerifyPackageTrustRequest {
            package: r.package.clone(),
            strict: r.strict,
        })
    }
    fn inspect_contract(&self, p: &str, c: &str) -> OperationEnvelope<Contract> {
        self.inspect_contract(p, c)
    }
    fn put_contract(&self, r: &PutContractInput) -> OperationEnvelope<ContractSummary> {
        self.put_contract(
            &r.package,
            &r.contract,
            &r.source,
            r.expected_fingerprint.as_deref(),
        )
    }
    fn delete_contract(&self, r: &DeleteContractInput) -> OperationEnvelope<ContractSummary> {
        self.delete_contract(&DeleteContractRequest {
            package: r.package.clone(),
            contract: r.contract.clone(),
            expected_fingerprint: r.expected_fingerprint.clone(),
        })
    }
    fn validate_contract(&self, p: &str, c: &str) -> OperationEnvelope<ContractSummary> {
        self.validate_contract(p, c)
    }
    fn validate_package(&self, p: &str) -> OperationEnvelope<Vec<ContractSummary>> {
        self.validate_package(p)
    }
    fn compile_contract(&self, r: &InteractionInput) -> OperationEnvelope<CompiledInteraction> {
        self.compile_contract(
            &r.package,
            &r.contract,
            &r.render_request(),
            &r.capabilities,
        )
    }
    fn render_contract(&self, r: &InteractionInput) -> OperationEnvelope<Vec<CompiledMessage>> {
        self.render_contract(
            &r.package,
            &r.contract,
            &r.render_request(),
            &r.capabilities,
        )
    }
    fn execute_contract(&self, r: &ExecuteContractInput) -> OperationEnvelope<ExecutionReceipt> {
        self.execute_contract(
            &r.interaction.package,
            &r.interaction.contract,
            &r.interaction.render_request(),
            &r.interaction.capabilities,
            r.fixture_output.clone(),
            r.stream,
        )
    }
    fn migrate_legacy(&self, r: &MigrateLegacyInput) -> OperationEnvelope<MigrationResult> {
        self.migrate_legacy(&MigrateLegacyRequest {
            package: r.package.clone(),
            dialect: r.dialect.clone(),
            source: r.source.clone(),
            aliases: r.aliases.clone(),
        })
    }
    fn render_document(&self, r: &RenderDocumentInput) -> OperationEnvelope<RenderDocumentResult> {
        self.render_document(&RenderDocumentRequest {
            package: r.package.clone(),
            template: r.template.clone(),
            data: r.data.clone(),
            output: r.output.clone(),
            workspace: r.workspace.clone(),
        })
    }
    fn list_workspace_artifacts(
        &self,
        r: &ListWorkspaceArtifactsInput,
    ) -> OperationEnvelope<Vec<WorkspaceArtifact>> {
        self.list_workspace_artifacts(&ListWorkspaceArtifactsRequest {
            package: r.package.clone(),
            workspace: r.workspace.clone(),
            prefix: r.prefix.clone(),
        })
    }
    fn read_artifact(&self, r: &ReadArtifactInput) -> OperationEnvelope<ArtifactContent> {
        self.read_artifact(&ReadArtifactRequest {
            package: r.package.clone(),
            path: r.path.clone(),
            workspace: r.workspace.clone(),
        })
    }
    fn delete_workspace_artifact(
        &self,
        r: &DeleteWorkspaceArtifactInput,
    ) -> OperationEnvelope<WorkspaceArtifact> {
        self.delete_workspace_artifact(&DeleteWorkspaceArtifactRequest {
            package: r.package.clone(),
            path: r.path.clone(),
            workspace: r.workspace.clone(),
            expected_fingerprint: r.expected_fingerprint.clone(),
        })
    }
    fn test_package(&self, p: &str, c: &[String]) -> OperationEnvelope<TestReport> {
        self.test_package(p, c)
    }
    fn list_evals(&self, p: &str) -> OperationEnvelope<Vec<templiqx_application::EvalCase>> {
        self.list_evals(p)
    }
    fn run_eval(&self, r: &RunEvalInput) -> OperationEnvelope<TestCaseResult> {
        self.run_eval(&r.package, &r.contract, &r.fixture_id, &r.capabilities)
    }
    fn diff_contract(&self, r: &DiffContractInput) -> OperationEnvelope<ContractDiff> {
        self.diff_contract(
            &r.left_package,
            &r.left_contract,
            &r.right_package,
            &r.right_contract,
        )
    }
    fn explain_contract(&self, p: &str, c: &str) -> OperationEnvelope<Explanation> {
        self.explain_contract(p, c)
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContractRefInput {
    pub package: String,
    pub contract: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageInput {
    pub package: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePackageInput {
    pub name: String,
    pub version: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePackageInput {
    pub package: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub expected_fingerprint: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeletePackageInput {
    pub package: String,
    pub expected_fingerprint: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SignPackageInput {
    pub package: String,
    pub key_id: String,
    pub expected_fingerprint: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyPackageTrustInput {
    pub package: String,
    #[serde(default)]
    pub strict: bool,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteContractInput {
    pub package: String,
    pub contract: String,
    pub expected_fingerprint: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PutContractInput {
    pub package: String,
    pub contract: String,
    pub source: String,
    /// Compare-and-swap fingerprint; omit only when creating.
    pub expected_fingerprint: Option<String>,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InteractionInput {
    pub package: String,
    pub contract: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}
impl InteractionInput {
    fn render_request(&self) -> RenderRequest {
        RenderRequest {
            inputs: self.inputs.clone(),
            context: self.context.clone(),
        }
    }
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecuteContractInput {
    #[serde(flatten)]
    #[schemars(flatten)]
    pub interaction: InteractionInput,
    /// Output supplied to the deterministic fake runtime.
    pub fixture_output: Option<Value>,
    /// Drive the streaming runtime path. The receipt is identical to a
    /// non-streaming execution; only the transport differs.
    #[serde(default)]
    pub stream: bool,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MigrateLegacyInput {
    pub package: String,
    pub dialect: String,
    /// Portable path relative to the package root.
    pub source: String,
    #[serde(default)]
    pub aliases: Value,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderDocumentInput {
    pub package: String,
    /// Portable input path relative to the package root.
    pub template: String,
    pub data: Value,
    /// Portable output path relative to the workspace root.
    pub output: String,
    #[serde(default)]
    pub workspace: Option<String>,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListWorkspaceArtifactsInput {
    pub package: String,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadArtifactInput {
    pub package: String,
    pub path: String,
    #[serde(default)]
    pub workspace: Option<String>,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteWorkspaceArtifactInput {
    pub package: String,
    pub path: String,
    #[serde(default)]
    pub workspace: Option<String>,
    pub expected_fingerprint: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestPackageInput {
    pub package: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiffContractInput {
    pub left_package: String,
    pub left_contract: String,
    pub right_package: String,
    pub right_contract: String,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunEvalInput {
    pub package: String,
    pub contract: String,
    pub fixture_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Typed structured-content envelope. Application result DTOs are retained as JSON.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StructuredEnvelope {
    pub api_version: String,
    pub operation: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    pub diagnostics: Vec<StructuredDiagnostic>,
    pub fingerprints: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub stream_events: Vec<StreamEvent>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StructuredDiagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_pointer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<StructuredSourceSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StructuredSourceSpan {
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl StructuredEnvelope {
    fn from_operation<T: Serialize>(value: OperationEnvelope<T>) -> Self {
        Self {
            api_version: value.api_version,
            operation: value.operation,
            ok: value.ok,
            result: value.result.and_then(|v| serde_json::to_value(v).ok()),
            diagnostics: value
                .diagnostics
                .into_iter()
                .map(|d| StructuredDiagnostic {
                    code: d.code,
                    severity: match d.severity {
                        templiqx_contracts::Severity::Error => "error",
                        templiqx_contracts::Severity::Warning => "warning",
                        templiqx_contracts::Severity::Info => "info",
                    }
                    .to_owned(),
                    message: d.message,
                    file: d.file,
                    json_pointer: d.json_pointer,
                    span: d.span.map(|s| StructuredSourceSpan {
                        line: s.line,
                        column: s.column,
                        end_line: s.end_line,
                        end_column: s.end_column,
                    }),
                    help: d.help,
                })
                .collect(),
            fingerprints: value.fingerprints,
            stream_events: value.stream_events,
        }
    }
}

#[derive(Clone)]
pub struct TempliqxMcp {
    operations: Arc<dyn Operations>,
    packages_root: Option<PathBuf>,
    workspace_root: Option<PathBuf>,
    #[allow(dead_code)] // rmcp's generated handler accesses the router through generated code.
    tool_router: ToolRouter<Self>,
}
impl TempliqxMcp {
    #[must_use]
    pub fn new(operations: impl Operations) -> Self {
        Self::from_arc(Arc::new(operations))
    }
    #[must_use]
    pub fn with_packages_root(mut self, root: PathBuf) -> Self {
        self.packages_root = Some(root);
        self
    }
    #[must_use]
    pub fn with_workspace_root(mut self, root: PathBuf) -> Self {
        self.workspace_root = Some(root);
        self
    }
    #[must_use]
    pub fn from_arc(operations: Arc<dyn Operations>) -> Self {
        Self {
            operations,
            packages_root: None,
            workspace_root: None,
            tool_router: Self::tool_router(),
        }
    }

    fn agent_instructions(&self) -> String {
        let mut lines = vec![
            "Templiqx actor-neutral AI contract compiler (MCP).".into(),
            "Suggested flow: discover_packages → validate_package → compile_contract → execute_contract.".into(),
            "After render_document, call list_workspace_artifacts then read_artifact to inspect outputs.".into(),
            "Use create_package to bootstrap an empty package when none exist.".into(),
            "Expected validation failures are structured operation diagnostics.".into(),
        ];
        if let Some(root) = &self.packages_root {
            lines.push(format!("Packages root: {}", root.display()));
            if let Ok(store) = templiqx_local::FilesystemPackageStore::new(root) {
                match store.discover() {
                    Ok(manifests) if manifests.is_empty() => {
                        lines.push(
                            "No packages discovered yet — call create_package to bootstrap one."
                                .into(),
                        );
                    }
                    Ok(manifests) => {
                        let names: Vec<_> = manifests.iter().map(|m| m.package.as_str()).collect();
                        lines.push(format!("Discovered packages: {}", names.join(", ")));
                    }
                    Err(_) => lines.push(
                        "Package discovery failed at initialize — call discover_packages.".into(),
                    ),
                }
            }
        }
        if let Some(root) = &self.workspace_root {
            lines.push(format!("Workspace root: {}", root.display()));
        }
        lines.push("Workspace writes are package-scoped and traversal/symlink confined.".into());
        lines.join("\n")
    }

    fn catalog_resource_text(&self) -> Result<String, McpError> {
        let envelope = StructuredEnvelope::from_operation(self.operations.catalog());
        serde_json::to_string(&envelope)
            .map_err(|error| McpError::internal_error(error.to_string(), None))
    }

    fn packages_resource_text(&self) -> Result<String, McpError> {
        let envelope = StructuredEnvelope::from_operation(self.operations.discover_packages());
        serde_json::to_string(&envelope)
            .map_err(|error| McpError::internal_error(error.to_string(), None))
    }

    fn workspace_resource_text(&self) -> Result<String, McpError> {
        serde_json::to_string(&serde_json::json!({
            "workspace": self.workspace_root.as_ref().map(|path| path.display().to_string()),
            "default": self.workspace_root.is_none(),
            "safety": "package-scoped; absolute artifact paths, traversal, backslashes and symlink escapes are rejected"
        }))
        .map_err(|error| McpError::internal_error(error.to_string(), None))
    }
}

#[tool_router]
impl TempliqxMcp {
    #[tool(description = "List canonical Templiqx capabilities and introspect the catalog")]
    fn catalog(&self) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.catalog(),
        ))
    }
    #[tool(description = "Discover portable Templiqx packages under the configured root")]
    fn discover_packages(&self) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.discover_packages(),
        ))
    }
    #[tool(description = "Bootstrap an empty portable package with templiqx.yaml and contracts/")]
    fn create_package(
        &self,
        Parameters(i): Parameters<CreatePackageInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.create_package(&i),
        ))
    }
    #[tool(description = "Update package version/description with CAS; invalidates signatures")]
    fn update_package(
        &self,
        Parameters(i): Parameters<UpdatePackageInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.update_package(&i),
        ))
    }
    #[tool(description = "Delete a package with CAS, dependency and untracked-content safety")]
    fn delete_package(
        &self,
        Parameters(i): Parameters<DeletePackageInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.delete_package(&i),
        ))
    }
    #[tool(description = "Export canonical signature-free manifest plus sorted artifact hashes")]
    fn export_package_identity(
        &self,
        Parameters(i): Parameters<PackageInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.export_package_identity(&i.package),
        ))
    }
    #[tool(description = "Attach a local dev/CI signature using the server environment key")]
    fn sign_package(
        &self,
        Parameters(i): Parameters<SignPackageInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.sign_package(&i),
        ))
    }
    #[tool(description = "Verify package trust; strict mode rejects unsigned packages")]
    fn verify_package_trust(
        &self,
        Parameters(i): Parameters<VerifyPackageTrustInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.verify_package_trust(&i),
        ))
    }
    #[tool(description = "Inspect one canonical contract")]
    fn inspect_contract(
        &self,
        Parameters(i): Parameters<ContractRefInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.inspect_contract(&i.package, &i.contract),
        ))
    }
    #[tool(description = "Create or compare-and-swap update one canonical contract")]
    fn put_contract(
        &self,
        Parameters(i): Parameters<PutContractInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.put_contract(&i),
        ))
    }
    #[tool(description = "Delete one contract with compare-and-swap fingerprint safety")]
    fn delete_contract(
        &self,
        Parameters(i): Parameters<DeleteContractInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.delete_contract(&i),
        ))
    }
    #[tool(description = "Validate one canonical contract")]
    fn validate_contract(
        &self,
        Parameters(i): Parameters<ContractRefInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.validate_contract(&i.package, &i.contract),
        ))
    }
    #[tool(description = "Validate a complete portable package")]
    fn validate_package(
        &self,
        Parameters(i): Parameters<PackageInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.validate_package(&i.package),
        ))
    }
    #[tool(description = "Compile a contract into one provider-neutral model interaction")]
    fn compile_contract(
        &self,
        Parameters(i): Parameters<InteractionInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.compile_contract(&i),
        ))
    }
    #[tool(description = "Deterministically render contract messages without model execution")]
    fn render_contract(
        &self,
        Parameters(i): Parameters<InteractionInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.render_contract(&i),
        ))
    }
    #[tool(
        description = "Execute one contract; set stream=true to collect stream_events in the envelope"
    )]
    fn execute_contract(
        &self,
        Parameters(i): Parameters<ExecuteContractInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.execute_contract(&i),
        ))
    }
    #[tool(description = "Migrate an explicitly identified supported legacy dialect")]
    fn migrate_legacy(
        &self,
        Parameters(i): Parameters<MigrateLegacyInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.migrate_legacy(&i),
        ))
    }
    #[tool(description = "Render a supported document template")]
    fn render_document(
        &self,
        Parameters(i): Parameters<RenderDocumentInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.render_document(&i),
        ))
    }
    #[tool(
        description = "List workspace artifact paths after render_document or execution outputs"
    )]
    fn list_workspace_artifacts(
        &self,
        Parameters(i): Parameters<ListWorkspaceArtifactsInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.list_workspace_artifacts(&i),
        ))
    }
    #[tool(description = "Read one workspace artifact (UTF-8 text or base64 for binary)")]
    fn read_artifact(
        &self,
        Parameters(i): Parameters<ReadArtifactInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.read_artifact(&i),
        ))
    }
    #[tool(description = "Delete one confined workspace artifact with byte-fingerprint CAS")]
    fn delete_workspace_artifact(
        &self,
        Parameters(i): Parameters<DeleteWorkspaceArtifactInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.delete_workspace_artifact(&i),
        ))
    }
    #[tool(description = "Run all deterministic package eval fixtures")]
    fn test_package(
        &self,
        Parameters(i): Parameters<TestPackageInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.test_package(&i.package, &i.capabilities),
        ))
    }
    #[tool(description = "List every (contract, fixture) eval pair addressable by run_eval")]
    fn list_evals(&self, Parameters(i): Parameters<PackageInput>) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.list_evals(&i.package),
        ))
    }
    #[tool(description = "Run one eval fixture via the same path as test_package")]
    fn run_eval(&self, Parameters(i): Parameters<RunEvalInput>) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.run_eval(&i),
        ))
    }
    #[tool(description = "Diff two canonical contracts")]
    fn diff_contract(
        &self,
        Parameters(i): Parameters<DiffContractInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.diff_contract(&i),
        ))
    }
    #[tool(description = "Explain typed inputs, context, capabilities and components")]
    fn explain_contract(
        &self,
        Parameters(i): Parameters<ContractRefInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.explain_contract(&i.package, &i.contract),
        ))
    }
}

#[tool_handler]
impl ServerHandler for TempliqxMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_instructions(self.agent_instructions())
        .with_server_info(Implementation::new(
            "templiqx-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(RESOURCE_CATALOG_URI, "catalog")
                .with_description("Canonical Templiqx capability catalog")
                .with_mime_type("application/json"),
            Resource::new(RESOURCE_PACKAGES_URI, "packages")
                .with_description("Discovered portable package manifest summaries")
                .with_mime_type("application/json"),
            Resource::new(RESOURCE_WORKSPACE_URI, "workspace")
                .with_description("Configured writable artifact workspace and safety boundary")
                .with_mime_type("application/json"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let text = match request.uri.as_str() {
            RESOURCE_CATALOG_URI => self.catalog_resource_text()?,
            RESOURCE_PACKAGES_URI => self.packages_resource_text()?,
            RESOURCE_WORKSPACE_URI => self.workspace_resource_text()?,
            uri => {
                return Err(McpError::resource_not_found(
                    format!("unknown resource: {uri}"),
                    None,
                ));
            }
        };
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri).with_mime_type("application/json"),
        ]))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult::with_all_items(vec![
            Prompt::new(
                "bootstrap",
                Some("Discover or create and validate a Templiqx package"),
                Some(vec![PromptArgument::new("package").with_required(true)]),
            ),
            Prompt::new(
                "run-eval",
                Some("List and run one deterministic package eval"),
                Some(vec![PromptArgument::new("package").with_required(true)]),
            ),
        ]))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let package = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("package"))
            .and_then(Value::as_str)
            .unwrap_or("PACKAGE");
        let text = match request.name.as_str() {
            "bootstrap" => format!(
                "Call discover_packages. If '{package}' is absent, call create_package, then validate_package. Keep package sources separate from the writable workspace."
            ),
            "run-eval" => format!(
                "Call list_evals for '{package}', choose one returned contract_id/fixture_id pair, then call run_eval with that exact pair."
            ),
            name => {
                return Err(McpError::invalid_params(
                    format!("unknown prompt: {name}"),
                    None,
                ));
            }
        };
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            text,
        )]))
    }
}

/// Serve MCP exclusively over stdin/stdout. Keep all logging on stderr.
pub async fn serve_stdio(server: TempliqxMcp) -> anyhow::Result<()> {
    use rmcp::ServiceExt as _;
    server
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::{
        ServiceExt as _,
        model::{CallToolRequestParams, JsonObject, ReadResourceRequestParams},
    };
    use serde_json::json;

    async fn client(
        root: &std::path::Path,
    ) -> anyhow::Result<(
        rmcp::service::RunningService<rmcp::RoleClient, ()>,
        tokio::task::JoinHandle<anyhow::Result<()>>,
    )> {
        let application = templiqx_local::compose(root)?;
        let packages_root = root.to_owned();
        let workspace_root = root.join(".templiqx-workspace");
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            let service = TempliqxMcp::new(application)
                .with_packages_root(packages_root)
                .with_workspace_root(workspace_root)
                .serve(server_transport)
                .await?;
            service.waiting().await?;
            anyhow::Ok(())
        });
        Ok((().serve(client_transport).await?, server_task))
    }

    fn arguments(value: Value) -> JsonObject {
        serde_json::from_value(value).expect("test arguments are an object")
    }

    #[tokio::test]
    async fn initializes_and_lists_the_exact_typed_catalog() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let (client, server_task) = client(temp.path()).await?;

        let info = client.peer_info().expect("initialize handshake completed");
        assert!(info.capabilities.tools.is_some());
        assert!(info.capabilities.resources.is_some());
        assert!(info.capabilities.prompts.is_some());
        assert!(
            info.instructions
                .as_deref()
                .is_some_and(|text| text.len() > 200),
            "onboarding instructions should be substantive"
        );
        assert_eq!(info.server_info.name, "templiqx-mcp");

        let listed = client.peer().list_tools(None).await?;
        let names: Vec<_> = listed.tools.iter().map(|tool| tool.name.as_ref()).collect();
        let mut expected = TOOL_CATALOG.to_vec();
        expected.sort_unstable();
        assert_eq!(names, expected);
        for tool in &listed.tools {
            assert_eq!(tool.input_schema.get("type"), Some(&json!("object")));
            assert!(
                tool.output_schema.is_some(),
                "{} lacks output schema",
                tool.name
            );
        }
        let compile = listed
            .tools
            .iter()
            .find(|tool| tool.name == "compile_contract")
            .unwrap();
        let properties = compile
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .unwrap();
        assert!(properties.contains_key("package"));
        assert!(properties.contains_key("inputs"));
        let output = compile.output_schema.as_ref().unwrap();
        assert!(
            output
                .get("properties")
                .and_then(Value::as_object)
                .unwrap()
                .contains_key("diagnostics")
        );

        client.cancel().await?;
        server_task.await??;
        Ok(())
    }

    #[tokio::test]
    async fn lists_and_reads_catalog_and_packages_resources() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        templiqx_local::create_package(temp.path(), "demo", "0.1.0")?;
        let (client, server_task) = client(temp.path()).await?;

        let listed = client.list_resources(None).await?;
        let uris: Vec<_> = listed
            .resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect();
        assert!(uris.contains(&RESOURCE_CATALOG_URI));
        assert!(uris.contains(&RESOURCE_PACKAGES_URI));
        assert!(uris.contains(&RESOURCE_WORKSPACE_URI));

        let workspace = client
            .read_resource(ReadResourceRequestParams::new(RESOURCE_WORKSPACE_URI))
            .await?;
        let workspace_text = workspace
            .contents
            .first()
            .and_then(|content| match content {
                ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .expect("workspace resource text");
        assert!(workspace_text.contains(".templiqx-workspace"));

        let prompts = client.list_prompts(None).await?;
        assert_eq!(
            prompts
                .prompts
                .iter()
                .map(|prompt| prompt.name.as_str())
                .collect::<Vec<_>>(),
            vec!["bootstrap", "run-eval"]
        );
        let prompt = client
            .get_prompt(rmcp::model::GetPromptRequestParams::new("bootstrap"))
            .await?;
        assert_eq!(prompt.messages.len(), 1);

        let catalog_tool = client
            .call_tool(CallToolRequestParams::new("catalog"))
            .await?;
        let catalog_resource = client
            .read_resource(ReadResourceRequestParams::new(RESOURCE_CATALOG_URI))
            .await?;
        let catalog_text = catalog_resource
            .contents
            .first()
            .and_then(|content| match content {
                ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
                ResourceContents::BlobResourceContents { .. } => None,
                _ => None,
            })
            .expect("catalog resource is text");
        let catalog_json: Value = serde_json::from_str(catalog_text)?;
        assert_eq!(
            catalog_json,
            catalog_tool
                .structured_content
                .expect("catalog tool structured content")
        );

        let packages_resource = client
            .read_resource(ReadResourceRequestParams::new(RESOURCE_PACKAGES_URI))
            .await?;
        let packages_text = packages_resource
            .contents
            .first()
            .and_then(|content| match content {
                ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
                ResourceContents::BlobResourceContents { .. } => None,
                _ => None,
            })
            .expect("packages resource is text");
        let packages_json: Value = serde_json::from_str(packages_text)?;
        assert_eq!(packages_json["operation"], "discover_packages");
        assert_eq!(packages_json["ok"], true);
        assert_eq!(packages_json["result"][0]["package"], "demo");

        client.cancel().await?;
        server_task.await??;
        Ok(())
    }

    #[tokio::test]
    async fn representative_calls_return_structured_envelopes_not_protocol_errors()
    -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        templiqx_local::create_package(temp.path(), "crm3", "0.1.0")?;
        let (client, server_task) = client(temp.path()).await?;

        let discovered = client
            .call_tool(CallToolRequestParams::new("discover_packages"))
            .await?;
        assert_ne!(discovered.is_error, Some(true));
        let content = discovered
            .structured_content
            .expect("typed structured content");
        assert_eq!(content["operation"], "discover_packages");
        assert_eq!(content["ok"], true);
        assert_eq!(content["result"][0]["package"], "crm3");

        let invalid = client
            .call_tool(
                CallToolRequestParams::new("validate_contract").with_arguments(arguments(json!({
                    "package": "crm3", "contract": "missing"
                }))),
            )
            .await?;
        assert_ne!(
            invalid.is_error,
            Some(true),
            "domain diagnostics are not MCP errors"
        );
        let content = invalid
            .structured_content
            .expect("typed structured content");
        assert_eq!(content["operation"], "validate_contract");
        assert_eq!(content["ok"], false);
        assert_eq!(content["diagnostics"][0]["code"], "TQX_NOT_FOUND");

        client.cancel().await?;
        server_task.await??;
        Ok(())
    }
}
