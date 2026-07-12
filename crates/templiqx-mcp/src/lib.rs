//! MCP stdio adapter over Templiqx's actor-neutral application capabilities.

use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars::{self, JsonSchema},
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use templiqx_application::{
    CreatePackageRequest, DeleteContractRequest, ListWorkspaceArtifactsRequest,
    MigrateLegacyRequest, MigrationResult, ReadArtifactRequest, RenderDocumentRequest,
    RenderDocumentResult, TempliqxService,
};
use templiqx_contracts::{
    ArtifactContent, CompiledInteraction, CompiledMessage, Contract, ContractDiff, ContractSummary,
    ExecutionReceipt, Explanation, OperationEnvelope, PackageManifest, RenderRequest, StreamEvent,
    TestCaseResult, TestReport, WorkspaceArtifact,
};
use templiqx_ports::{
    ArtifactWorkspace, DocumentRenderer, LegacyImportAdapter, PackageStore, RuntimeAdapter,
};

/// Stable MCP tool names, exactly matching the application catalog.
pub const TOOL_CATALOG: &[&str] = templiqx_application::CAPABILITY_CATALOG;

/// Object-safe routing view of the canonical application service.
pub trait Operations: Send + Sync + 'static {
    fn catalog(&self) -> OperationEnvelope<Vec<String>>;
    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>>;
    fn create_package(&self, request: &CreatePackageInput) -> OperationEnvelope<PackageManifest>;
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
    pub file: Option<String>,
    pub json_pointer: Option<String>,
    pub span: Option<StructuredSourceSpan>,
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
    pub fn from_arc(operations: Arc<dyn Operations>) -> Self {
        Self {
            operations,
            packages_root: None,
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
        lines.join("\n")
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
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(self.agent_instructions())
            .with_server_info(Implementation::new(
                "templiqx-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
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
        model::{CallToolRequestParams, JsonObject},
    };
    use serde_json::json;

    async fn client(
        root: &std::path::Path,
    ) -> anyhow::Result<(
        rmcp::service::RunningService<rmcp::RoleClient, ()>,
        tokio::task::JoinHandle<anyhow::Result<()>>,
    )> {
        let application = templiqx_local::compose(root)?;
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            let service = TempliqxMcp::new(application)
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
