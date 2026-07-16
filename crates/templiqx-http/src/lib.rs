//! Production-shaped northbound HTTP transport over the canonical Templiqx service.
//!
//! This crate is intentionally thin: every operation delegates to
//! `templiqx-application` through a host-injected service. It must not
//! depend on conformance mock crates or the mock gateway.

use axum::{
    Json, Router,
    body::Body,
    error_handling::HandleErrorLayer,
    extract::{
        DefaultBodyLimit, Path, Query, Request, State,
        rejection::{JsonRejection, QueryRejection, StringRejection},
    },
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    path::Path as FsPath,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use templiqx_application::{
    CreatePackageRequest, DeleteContractRequest, DeletePackageRequest,
    DeleteWorkspaceArtifactRequest, EvalCase, InspectDocumentRequest, InspectDocumentResult,
    ListWorkspaceArtifactsRequest, MigrateLegacyRequest, MigrationResult, ReadArtifactRequest,
    RenderDocumentRequest, RenderDocumentResult, SignPackageRequest, TempliqxService,
    UpdatePackageRequest, VerifyPackageTrustRequest,
};
use templiqx_contracts::{
    API_VERSION, ArtifactContent, CompiledInteraction, CompiledMessage, Contract, ContractDiff,
    ContractSummary, Diagnostic, ExecutionReceipt, Explanation, OperationEnvelope, PackageIdentity,
    PackageManifest, PackageTrustReport, RenderRequest, Severity, TestCaseResult, TestReport,
    WorkspaceArtifact,
};
use templiqx_ports::{
    ArtifactWorkspace, DocumentInspector, DocumentRenderer, LegacyImportAdapter, PackageStore,
    RuntimeAdapter,
};
use tower::{BoxError, ServiceBuilder, timeout::TimeoutLayer};
use utoipa_swagger_ui::{Config, SwaggerUi};

const OPENAPI_JSON_PATH: &str = "/operations/v1/openapi.json";
const SWAGGER_UI_PATH: &str = "/swagger-ui";

const MAX_BODY_BYTES: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
const OPENAPI_YAML: &str = include_str!("../../../openapi/templiqx-operations-v1.yaml");
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static OPENAPI_JSON: OnceLock<Value> = OnceLock::new();

pub trait HttpOperations: Send + Sync + 'static {
    fn catalog(&self) -> OperationEnvelope<Vec<String>>;
    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>>;
    fn create_package(&self, request: &CreatePackageRequest) -> OperationEnvelope<PackageManifest>;
    fn update_package(&self, request: &UpdatePackageRequest) -> OperationEnvelope<PackageManifest>;
    fn delete_package(&self, request: &DeletePackageRequest) -> OperationEnvelope<PackageManifest>;
    fn export_package_identity(&self, package: &str) -> OperationEnvelope<PackageIdentity>;
    fn sign_package(&self, request: &SignPackageRequest) -> OperationEnvelope<PackageManifest>;
    fn verify_package_trust(
        &self,
        request: &VerifyPackageTrustRequest,
    ) -> OperationEnvelope<PackageTrustReport>;
    fn inspect_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Contract>;
    fn put_contract(
        &self,
        package: &str,
        contract: &str,
        source: &str,
        expected_fingerprint: Option<&str>,
    ) -> OperationEnvelope<ContractSummary>;
    fn delete_contract(
        &self,
        request: &DeleteContractRequest,
    ) -> OperationEnvelope<ContractSummary>;
    fn validate_contract(
        &self,
        package: &str,
        contract: &str,
    ) -> OperationEnvelope<ContractSummary>;
    fn compile_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
    ) -> OperationEnvelope<CompiledInteraction>;
    fn execute_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
        fixture_output: Option<Value>,
        stream: bool,
    ) -> OperationEnvelope<ExecutionReceipt>;
    fn validate_package(&self, package: &str) -> OperationEnvelope<Vec<ContractSummary>>;
    fn render_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
    ) -> OperationEnvelope<Vec<CompiledMessage>>;
    fn test_package(&self, package: &str, capabilities: &[String])
    -> OperationEnvelope<TestReport>;
    fn list_evals(&self, package: &str) -> OperationEnvelope<Vec<EvalCase>>;
    fn run_eval(
        &self,
        package: &str,
        contract: &str,
        fixture_id: &str,
        capabilities: &[String],
    ) -> OperationEnvelope<TestCaseResult>;
    fn diff_contract(
        &self,
        left_package: &str,
        left: &str,
        right_package: &str,
        right: &str,
    ) -> OperationEnvelope<ContractDiff>;
    fn explain_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Explanation>;
    fn migrate_legacy(&self, request: &MigrateLegacyRequest) -> OperationEnvelope<MigrationResult>;
    fn render_document(
        &self,
        request: &RenderDocumentRequest,
    ) -> OperationEnvelope<RenderDocumentResult>;
    fn inspect_document(
        &self,
        request: &InspectDocumentRequest,
    ) -> OperationEnvelope<InspectDocumentResult>;
    fn list_workspace_artifacts(
        &self,
        request: &ListWorkspaceArtifactsRequest,
    ) -> OperationEnvelope<Vec<WorkspaceArtifact>>;
    fn read_artifact(&self, request: &ReadArtifactRequest) -> OperationEnvelope<ArtifactContent>;
    fn delete_workspace_artifact(
        &self,
        request: &DeleteWorkspaceArtifactRequest,
    ) -> OperationEnvelope<WorkspaceArtifact>;
}

impl<S, W, R, L, D, I> HttpOperations for TempliqxService<S, W, R, L, D, I>
where
    S: PackageStore + Send + Sync + 'static,
    W: ArtifactWorkspace + Send + Sync + 'static,
    R: RuntimeAdapter + Send + Sync + 'static,
    L: LegacyImportAdapter + Send + Sync + 'static,
    D: DocumentRenderer + Send + Sync + 'static,
    I: DocumentInspector + Send + Sync + 'static,
{
    fn catalog(&self) -> OperationEnvelope<Vec<String>> {
        self.catalog()
    }

    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>> {
        self.discover_packages()
    }

    fn create_package(&self, request: &CreatePackageRequest) -> OperationEnvelope<PackageManifest> {
        self.create_package(request)
    }

    fn update_package(&self, request: &UpdatePackageRequest) -> OperationEnvelope<PackageManifest> {
        self.update_package(request)
    }

    fn delete_package(&self, request: &DeletePackageRequest) -> OperationEnvelope<PackageManifest> {
        self.delete_package(request)
    }

    fn export_package_identity(&self, package: &str) -> OperationEnvelope<PackageIdentity> {
        self.export_package_identity(package)
    }

    fn sign_package(&self, request: &SignPackageRequest) -> OperationEnvelope<PackageManifest> {
        self.sign_package(request)
    }

    fn verify_package_trust(
        &self,
        request: &VerifyPackageTrustRequest,
    ) -> OperationEnvelope<PackageTrustReport> {
        self.verify_package_trust(request)
    }

    fn inspect_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Contract> {
        self.inspect_contract(package, contract)
    }

    fn put_contract(
        &self,
        package: &str,
        contract: &str,
        source: &str,
        expected_fingerprint: Option<&str>,
    ) -> OperationEnvelope<ContractSummary> {
        self.put_contract(package, contract, source, expected_fingerprint)
    }

    fn delete_contract(
        &self,
        request: &DeleteContractRequest,
    ) -> OperationEnvelope<ContractSummary> {
        self.delete_contract(request)
    }

    fn validate_contract(
        &self,
        package: &str,
        contract: &str,
    ) -> OperationEnvelope<ContractSummary> {
        self.validate_contract(package, contract)
    }

    fn compile_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
    ) -> OperationEnvelope<CompiledInteraction> {
        self.compile_contract(package, contract, request, capabilities)
    }

    fn execute_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
        fixture_output: Option<Value>,
        stream: bool,
    ) -> OperationEnvelope<ExecutionReceipt> {
        self.execute_contract(
            package,
            contract,
            request,
            capabilities,
            fixture_output,
            stream,
        )
    }

    fn validate_package(&self, package: &str) -> OperationEnvelope<Vec<ContractSummary>> {
        self.validate_package(package)
    }

    fn render_contract(
        &self,
        package: &str,
        contract: &str,
        request: &RenderRequest,
        capabilities: &[String],
    ) -> OperationEnvelope<Vec<CompiledMessage>> {
        self.render_contract(package, contract, request, capabilities)
    }

    fn test_package(
        &self,
        package: &str,
        capabilities: &[String],
    ) -> OperationEnvelope<TestReport> {
        self.test_package(package, capabilities)
    }

    fn list_evals(&self, package: &str) -> OperationEnvelope<Vec<EvalCase>> {
        self.list_evals(package)
    }

    fn run_eval(
        &self,
        package: &str,
        contract: &str,
        fixture_id: &str,
        capabilities: &[String],
    ) -> OperationEnvelope<TestCaseResult> {
        self.run_eval(package, contract, fixture_id, capabilities)
    }

    fn diff_contract(
        &self,
        left_package: &str,
        left: &str,
        right_package: &str,
        right: &str,
    ) -> OperationEnvelope<ContractDiff> {
        self.diff_contract(left_package, left, right_package, right)
    }

    fn explain_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Explanation> {
        self.explain_contract(package, contract)
    }

    fn migrate_legacy(&self, request: &MigrateLegacyRequest) -> OperationEnvelope<MigrationResult> {
        self.migrate_legacy(request)
    }

    fn render_document(
        &self,
        request: &RenderDocumentRequest,
    ) -> OperationEnvelope<RenderDocumentResult> {
        self.render_document(request)
    }

    fn inspect_document(
        &self,
        request: &InspectDocumentRequest,
    ) -> OperationEnvelope<InspectDocumentResult> {
        self.inspect_document(request)
    }

    fn list_workspace_artifacts(
        &self,
        request: &ListWorkspaceArtifactsRequest,
    ) -> OperationEnvelope<Vec<WorkspaceArtifact>> {
        self.list_workspace_artifacts(request)
    }

    fn read_artifact(&self, request: &ReadArtifactRequest) -> OperationEnvelope<ArtifactContent> {
        self.read_artifact(request)
    }

    fn delete_workspace_artifact(
        &self,
        request: &DeleteWorkspaceArtifactRequest,
    ) -> OperationEnvelope<WorkspaceArtifact> {
        self.delete_workspace_artifact(request)
    }
}

#[derive(Clone)]
pub struct HttpState {
    service: Arc<dyn HttpOperations>,
}

impl HttpState {
    #[must_use]
    pub fn new(service: impl HttpOperations) -> Self {
        Self {
            service: Arc::new(service),
        }
    }
}

/// Compose the local filesystem-backed service and return the HTTP router.
///
/// This is a local/dev convenience. Hosts that own production adapters should
/// compose their service externally and pass it to [`router`].
pub fn router_from_root(root: impl AsRef<FsPath>) -> Result<Router, templiqx_ports::PortError> {
    Ok(router(templiqx_local::compose(root)?))
}

/// Build a router over an injected service so hosts can supply their own adapters.
pub fn router(service: impl HttpOperations) -> Router {
    Router::new()
        .merge(swagger_ui())
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .route("/operations/v1/health/live", get(health))
        .route("/operations/v1/health/ready", get(ready))
        .route("/operations/v1/openapi.yaml", get(openapi_yaml))
        .route("/operations/v1/openapi.json", get(openapi_json))
        .route("/operations/v1/catalog", get(catalog))
        .route(
            "/operations/v1/packages",
            get(discover_packages).post(create_package),
        )
        .route(
            "/operations/v1/packages/{package}",
            patch(update_package).delete(delete_package),
        )
        .route(
            "/operations/v1/packages/{package}/validate",
            post(validate_package),
        )
        .route("/operations/v1/packages/{package}/test", post(test_package))
        .route(
            "/operations/v1/packages/{package}/identity",
            get(export_package_identity),
        )
        .route("/operations/v1/packages/{package}/sign", post(sign_package))
        .route(
            "/operations/v1/packages/{package}/verify-trust",
            post(verify_package_trust),
        )
        .route("/operations/v1/packages/{package}/evals", get(list_evals))
        .route(
            "/operations/v1/packages/{package}/evals/run",
            post(run_eval),
        )
        .route(
            "/operations/v1/packages/{package}/contracts/{contract}",
            get(inspect_contract)
                .put(put_contract)
                .delete(delete_contract),
        )
        .route(
            "/operations/v1/packages/{package}/contracts/{contract}/validate",
            post(validate_contract),
        )
        .route(
            "/operations/v1/packages/{package}/contracts/{contract}/compile",
            post(compile_contract),
        )
        .route(
            "/operations/v1/packages/{package}/contracts/{contract}/execute",
            post(execute_contract),
        )
        .route(
            "/operations/v1/packages/{package}/contracts/{contract}/render",
            post(render_contract),
        )
        .route(
            "/operations/v1/packages/{package}/contracts/{contract}/diff",
            post(diff_contract),
        )
        .route(
            "/operations/v1/packages/{package}/contracts/{contract}/explain",
            get(explain_contract),
        )
        .route("/operations/v1/legacy/migrate", post(migrate_legacy))
        .route("/operations/v1/documents/render", post(render_document))
        .route("/operations/v1/documents/inspect", post(inspect_document))
        .route("/operations/v1/artifacts", get(list_workspace_artifacts))
        .route(
            "/operations/v1/artifacts/{*artifact}",
            get(read_artifact).delete(delete_workspace_artifact),
        )
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn(request_id))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(timeout_error))
                .layer(TimeoutLayer::new(REQUEST_TIMEOUT)),
        )
        .with_state(HttpState::new(service))
}

/// Interactive Swagger UI for the checked-in Operations OpenAPI document.
///
/// Points the browser at [`OPENAPI_JSON_PATH`] so YAML remains the sole
/// wire-contract source of truth (no parallel utoipa-derived schema tree).
fn swagger_ui() -> SwaggerUi {
    SwaggerUi::new(SWAGGER_UI_PATH).config(Config::from(OPENAPI_JSON_PATH))
}

/// Bind `addr` and serve the router until SIGINT/CTRL+C, then drain in-flight requests.
pub async fn serve(router: Router, addr: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

/// Compose a local filesystem-backed service and serve it with graceful shutdown.
pub async fn serve_from_root(
    root: impl AsRef<FsPath>,
    addr: SocketAddr,
) -> Result<(), templiqx_ports::PortError> {
    serve(router_from_root(root)?, addr)
        .await
        .map_err(|error| templiqx_ports::PortError::Io(error.to_string()))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install SIGINT handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

async fn timeout_error(error: BoxError) -> Response {
    let message = if error.is::<tower::timeout::error::Elapsed>() {
        "request timed out"
    } else {
        "transport error"
    };
    envelope_response(OperationEnvelope::<Value>::new(
        "transport",
        None,
        vec![Diagnostic::error("TQX_HTTP_TRANSPORT", message, "/")],
    ))
}

async fn request_id(mut request: Request<Body>, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            format!("tqx-{id}")
        });
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(&REQUEST_ID_HEADER, value);
    }
    response
}

#[derive(Debug, Clone)]
struct RequestId(#[allow(dead_code)] String);

#[derive(serde::Serialize)]
struct Health<'a> {
    status: &'a str,
    api_version: &'a str,
}

async fn health() -> impl IntoResponse {
    Json(Health {
        status: "ok",
        api_version: API_VERSION,
    })
}

async fn ready(State(_state): State<HttpState>) -> impl IntoResponse {
    Json(Health {
        status: "ready",
        api_version: API_VERSION,
    })
}

async fn openapi_yaml() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/yaml")], OPENAPI_YAML)
}

async fn openapi_json() -> impl IntoResponse {
    Json(openapi_document_json())
}

fn openapi_document_json() -> &'static Value {
    OPENAPI_JSON.get_or_init(|| {
        let yaml: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(OPENAPI_YAML).expect("checked-in OpenAPI must parse as YAML");
        serde_json::to_value(yaml).expect("checked-in OpenAPI must convert to JSON")
    })
}

async fn catalog(State(state): State<HttpState>) -> Response {
    envelope_response(state.service.catalog())
}

async fn discover_packages(State(state): State<HttpState>) -> Response {
    envelope_response(state.service.discover_packages())
}

async fn create_package(
    State(state): State<HttpState>,
    body: Result<Json<CreatePackageRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("create_package", error),
    };
    envelope_response(state.service.create_package(&body))
}

async fn delete_package(
    State(state): State<HttpState>,
    Path(package): Path<String>,
    headers: HeaderMap,
) -> Response {
    let expected_fingerprint = match required_if_match(&headers, "delete_package") {
        Ok(value) => value,
        Err(response) => return *response,
    };
    envelope_response(state.service.delete_package(&DeletePackageRequest {
        package,
        expected_fingerprint,
    }))
}

async fn validate_package(State(state): State<HttpState>, Path(package): Path<String>) -> Response {
    envelope_response(state.service.validate_package(&package))
}

async fn test_package(
    State(state): State<HttpState>,
    Path(package): Path<String>,
    body: Result<Json<CapabilitiesBody>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("test_package", error),
    };
    envelope_response(state.service.test_package(&package, &body.capabilities))
}

async fn export_package_identity(
    State(state): State<HttpState>,
    Path(package): Path<String>,
) -> Response {
    envelope_response(state.service.export_package_identity(&package))
}

async fn sign_package(
    State(state): State<HttpState>,
    Path(package): Path<String>,
    headers: HeaderMap,
    body: Result<Json<SignPackageBody>, JsonRejection>,
) -> Response {
    let expected_fingerprint = match required_if_match(&headers, "sign_package") {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("sign_package", error),
    };
    envelope_response(state.service.sign_package(&SignPackageRequest {
        package,
        key_id: body.key_id,
        expected_fingerprint,
    }))
}

async fn verify_package_trust(
    State(state): State<HttpState>,
    Path(package): Path<String>,
    body: Result<Json<VerifyPackageTrustBody>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("verify_package_trust", error),
    };
    envelope_response(
        state
            .service
            .verify_package_trust(&VerifyPackageTrustRequest {
                package,
                strict: body.strict,
            }),
    )
}

async fn inspect_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
) -> Response {
    envelope_response(state.service.inspect_contract(&package, &contract))
}

async fn put_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
    headers: HeaderMap,
    source: Result<String, StringRejection>,
) -> Response {
    let source = match source {
        Ok(source) => source,
        Err(error) => return body_rejection("put_contract", error.to_string()),
    };
    let expected_fingerprint = optional_if_match(&headers);
    envelope_response(state.service.put_contract(
        &package,
        &contract,
        &source,
        expected_fingerprint.as_deref(),
    ))
}

async fn delete_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let expected_fingerprint = match required_if_match(&headers, "delete_contract") {
        Ok(value) => value,
        Err(response) => return *response,
    };
    envelope_response(state.service.delete_contract(&DeleteContractRequest {
        package,
        contract,
        expected_fingerprint,
    }))
}

async fn validate_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
) -> Response {
    envelope_response(state.service.validate_contract(&package, &contract))
}

async fn compile_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
    body: Result<Json<InteractionRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("compile_contract", error),
    };
    envelope_response(state.service.compile_contract(
        &package,
        &contract,
        &body.render,
        &body.capabilities,
    ))
}

async fn execute_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
    body: Result<Json<ExecuteRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("execute_contract", error),
    };
    envelope_response(state.service.execute_contract(
        &package,
        &contract,
        &body.interaction.render,
        &body.interaction.capabilities,
        body.fixture_output,
        body.stream,
    ))
}

async fn render_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
    body: Result<Json<InteractionRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("render_contract", error),
    };
    envelope_response(state.service.render_contract(
        &package,
        &contract,
        &body.render,
        &body.capabilities,
    ))
}

async fn run_eval(
    State(state): State<HttpState>,
    Path(package): Path<String>,
    body: Result<Json<RunEvalBody>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("run_eval", error),
    };
    envelope_response(state.service.run_eval(
        &package,
        &body.contract,
        &body.fixture_id,
        &body.capabilities,
    ))
}

async fn list_evals(State(state): State<HttpState>, Path(package): Path<String>) -> Response {
    envelope_response(state.service.list_evals(&package))
}

async fn diff_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
    body: Result<Json<DiffContractBody>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("diff_contract", error),
    };
    envelope_response(state.service.diff_contract(
        &package,
        &contract,
        &body.right_package,
        &body.right_contract,
    ))
}

async fn explain_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
) -> Response {
    envelope_response(state.service.explain_contract(&package, &contract))
}

async fn migrate_legacy(
    State(state): State<HttpState>,
    body: Result<Json<MigrateLegacyRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("migrate_legacy", error),
    };
    envelope_response(state.service.migrate_legacy(&body))
}

async fn render_document(
    State(state): State<HttpState>,
    body: Result<Json<RenderDocumentRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("render_document", error),
    };
    envelope_response(state.service.render_document(&body))
}

async fn inspect_document(
    State(state): State<HttpState>,
    body: Result<Json<InspectDocumentRequest>, JsonRejection>,
) -> Response {
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("inspect_document", error),
    };
    envelope_response(state.service.inspect_document(&body))
}

async fn list_workspace_artifacts(
    State(state): State<HttpState>,
    query: Result<Query<ListArtifactsQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(error) => return body_rejection("list_workspace_artifacts", error.to_string()),
    };
    envelope_response(
        state
            .service
            .list_workspace_artifacts(&ListWorkspaceArtifactsRequest {
                package: query.package,
                workspace: query.workspace,
                prefix: query.prefix,
            }),
    )
}

async fn read_artifact(
    State(state): State<HttpState>,
    Path(artifact): Path<String>,
    query: Result<Query<ArtifactQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(error) => return body_rejection("read_artifact", error.to_string()),
    };
    envelope_response(state.service.read_artifact(&ReadArtifactRequest {
        package: query.package,
        path: artifact,
        workspace: query.workspace,
    }))
}

async fn delete_workspace_artifact(
    State(state): State<HttpState>,
    Path(artifact): Path<String>,
    query: Result<Query<ArtifactQuery>, QueryRejection>,
    headers: HeaderMap,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(error) => return body_rejection("delete_workspace_artifact", error.to_string()),
    };
    let expected_fingerprint = match required_if_match(&headers, "delete_workspace_artifact") {
        Ok(value) => value,
        Err(response) => return *response,
    };
    envelope_response(
        state
            .service
            .delete_workspace_artifact(&DeleteWorkspaceArtifactRequest {
                package: query.package,
                path: artifact,
                workspace: query.workspace,
                expected_fingerprint,
            }),
    )
}

async fn update_package(
    State(state): State<HttpState>,
    Path(package): Path<String>,
    headers: HeaderMap,
    body: Result<Json<UpdatePackageBody>, JsonRejection>,
) -> Response {
    let expected_fingerprint = match required_if_match(&headers, "update_package") {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let Json(body) = match body {
        Ok(body) => body,
        Err(error) => return json_rejection("update_package", error),
    };
    envelope_response(state.service.update_package(&UpdatePackageRequest {
        package,
        version: body.version,
        description: body.description,
        expected_fingerprint,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InteractionRequest {
    #[serde(default = "empty_render_request")]
    render: RenderRequest,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecuteRequest {
    #[serde(flatten)]
    interaction: InteractionRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fixture_output: Option<Value>,
    #[serde(default)]
    stream: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdatePackageBody {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignPackageBody {
    key_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifyPackageTrustBody {
    #[serde(default)]
    strict: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilitiesBody {
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunEvalBody {
    contract: String,
    fixture_id: String,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiffContractBody {
    right_package: String,
    right_contract: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArtifactsQuery {
    package: String,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactQuery {
    package: String,
    #[serde(default)]
    workspace: Option<String>,
}

fn empty_render_request() -> RenderRequest {
    RenderRequest {
        inputs: BTreeMap::new(),
        context: BTreeMap::new(),
    }
}

fn json_rejection(operation: &str, error: JsonRejection) -> Response {
    body_rejection(operation, error.body_text())
}

fn body_rejection(operation: &str, message: String) -> Response {
    envelope_response(OperationEnvelope::<Value>::new(
        operation,
        None,
        vec![Diagnostic::error("TQX_HTTP_JSON", message, "/")],
    ))
}

fn optional_if_match(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_matches('"').to_owned())
}

#[allow(clippy::result_large_err)]
fn required_if_match(headers: &HeaderMap, operation: &str) -> Result<String, Box<Response>> {
    optional_if_match(headers).ok_or_else(|| {
        Box::new(envelope_response(OperationEnvelope::<Value>::new(
            operation,
            None,
            vec![Diagnostic::error(
                "TQX_HTTP_IF_MATCH_REQUIRED",
                "If-Match is required for this mutation",
                "/headers/if-match",
            )],
        )))
    })
}

fn envelope_response<T>(envelope: OperationEnvelope<T>) -> Response
where
    T: serde::Serialize,
{
    let status = status_for(&envelope);
    (status, Json(envelope)).into_response()
}

fn status_for<T>(envelope: &OperationEnvelope<T>) -> StatusCode {
    if envelope.ok {
        return StatusCode::OK;
    }
    let codes = envelope
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    if codes.iter().any(|code| code.contains("NOT_FOUND")) {
        StatusCode::NOT_FOUND
    } else if codes
        .iter()
        .any(|code| code.contains("CONFLICT") || code.contains("CAS"))
    {
        StatusCode::CONFLICT
    } else if codes
        .iter()
        .any(|code| code.starts_with("TQX_HTTP") || code.contains("INVALID"))
    {
        StatusCode::BAD_REQUEST
    } else if codes.iter().any(|code| code.contains("UNSUPPORTED")) {
        StatusCode::NOT_IMPLEMENTED
    } else if codes.iter().any(|code| code.contains("TIMEOUT")) {
        StatusCode::GATEWAY_TIMEOUT
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    }
}

// Compile-time shape checks for every route envelope result.
const _: fn(OperationEnvelope<Vec<String>>) = |_| {};
const _: fn(OperationEnvelope<Vec<PackageManifest>>) = |_| {};
const _: fn(OperationEnvelope<PackageManifest>) = |_| {};
const _: fn(OperationEnvelope<PackageIdentity>) = |_| {};
const _: fn(OperationEnvelope<PackageTrustReport>) = |_| {};
const _: fn(OperationEnvelope<Contract>) = |_| {};
const _: fn(OperationEnvelope<ContractSummary>) = |_| {};
const _: fn(OperationEnvelope<Vec<ContractSummary>>) = |_| {};
const _: fn(OperationEnvelope<CompiledInteraction>) = |_| {};
const _: fn(OperationEnvelope<Vec<CompiledMessage>>) = |_| {};
const _: fn(OperationEnvelope<ExecutionReceipt>) = |_| {};
const _: fn(OperationEnvelope<TestReport>) = |_| {};
const _: fn(OperationEnvelope<Vec<EvalCase>>) = |_| {};
const _: fn(OperationEnvelope<TestCaseResult>) = |_| {};
const _: fn(OperationEnvelope<ContractDiff>) = |_| {};
const _: fn(OperationEnvelope<Explanation>) = |_| {};
const _: fn(OperationEnvelope<MigrationResult>) = |_| {};
const _: fn(OperationEnvelope<RenderDocumentResult>) = |_| {};
const _: fn(OperationEnvelope<Vec<WorkspaceArtifact>>) = |_| {};
const _: fn(OperationEnvelope<ArtifactContent>) = |_| {};
const _: fn(OperationEnvelope<WorkspaceArtifact>) = |_| {};
