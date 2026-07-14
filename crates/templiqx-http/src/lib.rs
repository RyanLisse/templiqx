//! Production-shaped northbound HTTP transport over the canonical Templiqx service.
//!
//! This crate is intentionally thin: every operation delegates to
//! `templiqx-application` through a host-injected service. It must not
//! depend on conformance mock crates or the mock gateway.

use axum::{
    Json, Router,
    body::Body,
    error_handling::HandleErrorLayer,
    extract::{DefaultBodyLimit, Path, Request, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::Path as FsPath,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use templiqx_application::{TempliqxService, UpdatePackageRequest};
use templiqx_contracts::{
    API_VERSION, CompiledInteraction, Contract, ContractSummary, Diagnostic, ExecutionReceipt,
    OperationEnvelope, PackageManifest, RenderRequest, Severity,
};
use templiqx_ports::{
    ArtifactWorkspace, DocumentRenderer, LegacyImportAdapter, PackageStore, RuntimeAdapter,
};
use tower::{BoxError, ServiceBuilder, timeout::TimeoutLayer};

const MAX_BODY_BYTES: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

pub trait HttpOperations: Send + Sync + 'static {
    fn catalog(&self) -> OperationEnvelope<Vec<String>>;
    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>>;
    fn inspect_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Contract>;
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
    fn update_package(&self, request: &UpdatePackageRequest) -> OperationEnvelope<PackageManifest>;
}

impl<S, W, R, L, D> HttpOperations for TempliqxService<S, W, R, L, D>
where
    S: PackageStore + Send + Sync + 'static,
    W: ArtifactWorkspace + Send + Sync + 'static,
    R: RuntimeAdapter + Send + Sync + 'static,
    L: LegacyImportAdapter + Send + Sync + 'static,
    D: DocumentRenderer + Send + Sync + 'static,
{
    fn catalog(&self) -> OperationEnvelope<Vec<String>> {
        self.catalog()
    }

    fn discover_packages(&self) -> OperationEnvelope<Vec<PackageManifest>> {
        self.discover_packages()
    }

    fn inspect_contract(&self, package: &str, contract: &str) -> OperationEnvelope<Contract> {
        self.inspect_contract(package, contract)
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

    fn update_package(&self, request: &UpdatePackageRequest) -> OperationEnvelope<PackageManifest> {
        self.update_package(request)
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
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .route("/operations/v1/health/live", get(health))
        .route("/operations/v1/health/ready", get(ready))
        .route("/operations/v1/openapi.yaml", get(openapi))
        .route("/operations/v1/catalog", get(catalog))
        .route("/operations/v1/packages", get(discover_packages))
        .route(
            "/operations/v1/packages/{package}",
            axum::routing::patch(update_package),
        )
        .route(
            "/operations/v1/packages/{package}/contracts/{contract}",
            get(inspect_contract),
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
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn(request_id))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(timeout_error))
                .layer(TimeoutLayer::new(REQUEST_TIMEOUT)),
        )
        .with_state(HttpState::new(service))
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

async fn openapi() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/yaml")],
        include_str!("../../../openapi/templiqx-operations-v1.yaml"),
    )
}

async fn catalog(State(state): State<HttpState>) -> Response {
    envelope_response(state.service.catalog())
}

async fn discover_packages(State(state): State<HttpState>) -> Response {
    envelope_response(state.service.discover_packages())
}

async fn inspect_contract(
    State(state): State<HttpState>,
    Path((package, contract)): Path<(String, String)>,
) -> Response {
    envelope_response(state.service.inspect_contract(&package, &contract))
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

async fn update_package(
    State(state): State<HttpState>,
    Path(package): Path<String>,
    headers: HeaderMap,
    body: Result<Json<UpdatePackageBody>, JsonRejection>,
) -> Response {
    let expected_fingerprint = match headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.trim_matches('"').to_owned(),
        None => {
            return envelope_response(OperationEnvelope::<PackageManifest>::new(
                "update_package",
                None,
                vec![Diagnostic::error(
                    "TQX_HTTP_IF_MATCH_REQUIRED",
                    "If-Match is required for package updates",
                    "/headers/if-match",
                )],
            ));
        }
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

fn empty_render_request() -> RenderRequest {
    RenderRequest {
        inputs: BTreeMap::new(),
        context: BTreeMap::new(),
    }
}

fn json_rejection(operation: &str, error: JsonRejection) -> Response {
    envelope_response(OperationEnvelope::<Value>::new(
        operation,
        None,
        vec![Diagnostic::error("TQX_HTTP_JSON", error.body_text(), "/")],
    ))
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

// Compile-time shape checks for representative route envelopes.
const _: fn(OperationEnvelope<Vec<String>>) = |_| {};
const _: fn(OperationEnvelope<Vec<PackageManifest>>) = |_| {};
const _: fn(OperationEnvelope<Contract>) = |_| {};
const _: fn(OperationEnvelope<ContractSummary>) = |_| {};
const _: fn(OperationEnvelope<CompiledInteraction>) = |_| {};
const _: fn(OperationEnvelope<ExecutionReceipt>) = |_| {};
