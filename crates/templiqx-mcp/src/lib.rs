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
use std::{collections::BTreeMap, sync::Arc};
use templiqx_application::{
    MigrateLegacyRequest, MigrationResult, RenderDocumentRequest, RenderDocumentResult,
    TempliqxService,
};
use templiqx_contracts::{
    CompiledInteraction, CompiledMessage, Contract, ContractDiff, ContractSummary,
    ExecutionReceipt, Explanation, OperationEnvelope, PackageManifest, RenderRequest, TestReport,
};
use templiqx_ports::{DocumentRenderer, LegacyImportAdapter, PackageStore, RuntimeAdapter};

/// Stable MCP tool names, exactly matching the application catalog.
pub const TOOL_CATALOG: &[&str] = templiqx_application::CAPABILITY_CATALOG;

/// Object-safe routing view of the canonical application service.
pub trait Operations: Send + Sync + 'static {
    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>>;
    fn inspect_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Contract>;
    fn put_contract(&self, request: &PutContractInput) -> OperationEnvelope<ContractSummary>;
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
    fn test_package(&self, package: &str, capabilities: &[String])
    -> OperationEnvelope<TestReport>;
    fn diff_contract(&self, request: &DiffContractInput) -> OperationEnvelope<ContractDiff>;
    fn explain_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Explanation>;
}

impl<S, R, L, D> Operations for TempliqxService<S, R, L, D>
where
    S: PackageStore + 'static,
    R: RuntimeAdapter + 'static,
    L: LegacyImportAdapter + 'static,
    D: DocumentRenderer + 'static,
{
    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>> {
        self.discover_packages()
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
        })
    }
    fn test_package(&self, p: &str, c: &[String]) -> OperationEnvelope<TestReport> {
        self.test_package(p, c)
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
    /// Portable output path relative to the package root.
    pub output: String,
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

/// Typed structured-content envelope. Application result DTOs are retained as JSON.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StructuredEnvelope {
    pub api_version: String,
    pub operation: String,
    pub ok: bool,
    pub result: Option<Value>,
    pub diagnostics: Vec<StructuredDiagnostic>,
    pub fingerprints: BTreeMap<String, String>,
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
        }
    }
}

#[derive(Clone)]
pub struct TempliqxMcp {
    operations: Arc<dyn Operations>,
    #[allow(dead_code)] // rmcp's generated handler accesses the router through generated code.
    tool_router: ToolRouter<Self>,
}
impl TempliqxMcp {
    #[must_use]
    pub fn new(operations: impl Operations) -> Self {
        Self::from_arc(Arc::new(operations))
    }
    #[must_use]
    pub fn from_arc(operations: Arc<dyn Operations>) -> Self {
        Self {
            operations,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl TempliqxMcp {
    #[tool(description = "Discover portable Templiqx packages")]
    fn discover_packages(&self) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.discover_packages(),
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
    #[tool(description = "Execute one contract with the deterministic fake runtime")]
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
    #[tool(description = "Run all deterministic package eval fixtures")]
    fn test_package(
        &self,
        Parameters(i): Parameters<TestPackageInput>,
    ) -> Json<StructuredEnvelope> {
        Json(StructuredEnvelope::from_operation(
            self.operations.test_package(&i.package, &i.capabilities),
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
            .with_instructions("Templiqx actor-neutral AI contract capabilities. Expected validation failures are structured operation diagnostics.")
            .with_server_info(Implementation::new("templiqx-mcp", env!("CARGO_PKG_VERSION")))
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
