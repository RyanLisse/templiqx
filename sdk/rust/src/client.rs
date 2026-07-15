//! Async `reqwest` façade over the generated wire DTOs.

use std::{
    collections::hash_map::RandomState,
    fmt::Write as _,
    hash::BuildHasher,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Method, StatusCode, header::HeaderMap};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    errors::{TempliqxError, TempliqxHttpError, TempliqxTransportError},
    generated::{
        CapabilitiesRequest, CatalogEnvelope, CompileRequest, CompiledInteractionEnvelope,
        ContractEnvelope, CreatePackageRequest, DiffContractRequest, ExecuteRequest,
        ExecutionReceiptEnvelope, HealthStatus, InspectDocumentEnvelope, InspectDocumentRequest,
        JsonValueEnvelope, MigrateLegacyRequest, PackageEnvelope, PackageListEnvelope,
        RenderDocumentRequest, RunEvalRequest, SignPackageRequest, SummaryEnvelope,
        UpdatePackageRequest, VerifyPackageTrustRequest,
    },
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
static UUID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Options applied to every request made by a client.
#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub timeout: Duration,
    pub default_headers: HeaderMap,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            default_headers: HeaderMap::new(),
        }
    }
}

/// Per-call transport options. `if_match` is honored only by CAS-capable operations.
#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    pub timeout: Option<Duration>,
    pub request_id: Option<String>,
    pub if_match: Option<String>,
}

/// Per-call options for operations that require an `If-Match` value.
#[derive(Debug, Clone)]
pub struct CasCallOptions {
    pub timeout: Option<Duration>,
    pub request_id: Option<String>,
    pub if_match: String,
}

impl From<CasCallOptions> for CallOptions {
    fn from(value: CasCallOptions) -> Self {
        Self {
            timeout: value.timeout,
            request_id: value.request_id,
            if_match: Some(value.if_match),
        }
    }
}

/// A typed response plus the request correlation ID returned by the server.
#[derive(Debug, Clone)]
pub struct TempliqxResponse<T> {
    pub data: T,
    pub request_id: String,
}

/// Thin async client for all Operations API operation IDs.
#[derive(Debug, Clone)]
pub struct Client {
    base_url: String,
    http: reqwest::Client,
    timeout: Duration,
}

impl Client {
    /// Construct a client. The base URL may include a trailing slash.
    pub fn new(
        base_url: impl Into<String>,
        options: ClientOptions,
    ) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .default_headers(options.default_headers)
            .timeout(options.timeout)
            .build()?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            http,
            timeout: options.timeout,
        })
    }

    pub async fn get_operations_v1_liveness(
        &self,
        options: CallOptions,
    ) -> Result<TempliqxResponse<HealthStatus>, TempliqxError> {
        self.send_json::<HealthStatus, ()>(
            Method::GET,
            "/operations/v1/health/live",
            None,
            options,
            false,
        )
        .await
    }

    pub async fn get_operations_v1_readiness(
        &self,
        options: CallOptions,
    ) -> Result<TempliqxResponse<HealthStatus>, TempliqxError> {
        self.send_json::<HealthStatus, ()>(
            Method::GET,
            "/operations/v1/health/ready",
            None,
            options,
            false,
        )
        .await
    }

    pub async fn get_operations_v1_open_api_yaml(
        &self,
        options: CallOptions,
    ) -> Result<TempliqxResponse<String>, TempliqxError> {
        self.send_text(Method::GET, "/operations/v1/openapi.yaml", options, false)
            .await
    }

    pub async fn get_operations_v1_open_api(
        &self,
        options: CallOptions,
    ) -> Result<TempliqxResponse<serde_json::Value>, TempliqxError> {
        self.send_json::<serde_json::Value, ()>(
            Method::GET,
            "/operations/v1/openapi.json",
            None,
            options,
            false,
        )
        .await
    }

    pub async fn catalog(
        &self,
        options: CallOptions,
    ) -> Result<TempliqxResponse<CatalogEnvelope>, TempliqxError> {
        self.send_json::<CatalogEnvelope, ()>(
            Method::GET,
            "/operations/v1/catalog",
            None,
            options,
            false,
        )
        .await
    }

    pub async fn discover_packages(
        &self,
        options: CallOptions,
    ) -> Result<TempliqxResponse<PackageListEnvelope>, TempliqxError> {
        self.send_json::<PackageListEnvelope, ()>(
            Method::GET,
            "/operations/v1/packages",
            None,
            options,
            false,
        )
        .await
    }

    pub async fn create_package(
        &self,
        body: &CreatePackageRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<PackageEnvelope>, TempliqxError> {
        self.send_json(
            Method::POST,
            "/operations/v1/packages",
            Some(body),
            options,
            false,
        )
        .await
    }

    pub async fn inspect_contract(
        &self,
        package: &str,
        contract: &str,
        options: CallOptions,
    ) -> Result<TempliqxResponse<ContractEnvelope>, TempliqxError> {
        let path = contract_path(package, contract);
        self.send_json::<ContractEnvelope, ()>(Method::GET, &path, None, options, false)
            .await
    }

    pub async fn put_contract(
        &self,
        package: &str,
        contract: &str,
        source: &str,
        options: CallOptions,
    ) -> Result<TempliqxResponse<SummaryEnvelope>, TempliqxError> {
        let path = contract_path(package, contract);
        self.send_yaml(Method::PUT, &path, source, options, true)
            .await
    }

    pub async fn delete_contract(
        &self,
        package: &str,
        contract: &str,
        options: CasCallOptions,
    ) -> Result<TempliqxResponse<SummaryEnvelope>, TempliqxError> {
        let path = contract_path(package, contract);
        self.send_json::<SummaryEnvelope, ()>(Method::DELETE, &path, None, options.into(), true)
            .await
    }

    pub async fn validate_contract(
        &self,
        package: &str,
        contract: &str,
        options: CallOptions,
    ) -> Result<TempliqxResponse<SummaryEnvelope>, TempliqxError> {
        let path = format!("{}/validate", contract_path(package, contract));
        self.send_json::<SummaryEnvelope, ()>(Method::POST, &path, None, options, false)
            .await
    }

    pub async fn compile_contract(
        &self,
        package: &str,
        contract: &str,
        body: &CompileRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<CompiledInteractionEnvelope>, TempliqxError> {
        let path = format!("{}/compile", contract_path(package, contract));
        self.send_json(Method::POST, &path, Some(body), options, false)
            .await
    }

    pub async fn execute_contract(
        &self,
        package: &str,
        contract: &str,
        body: &ExecuteRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<ExecutionReceiptEnvelope>, TempliqxError> {
        let path = format!("{}/execute", contract_path(package, contract));
        self.send_json(Method::POST, &path, Some(body), options, false)
            .await
    }

    pub async fn update_package(
        &self,
        package: &str,
        body: &UpdatePackageRequest,
        options: CasCallOptions,
    ) -> Result<TempliqxResponse<PackageEnvelope>, TempliqxError> {
        let path = package_path(package);
        self.send_json(Method::PATCH, &path, Some(body), options.into(), true)
            .await
    }

    pub async fn delete_package(
        &self,
        package: &str,
        options: CasCallOptions,
    ) -> Result<TempliqxResponse<PackageEnvelope>, TempliqxError> {
        let path = package_path(package);
        self.send_json::<PackageEnvelope, ()>(Method::DELETE, &path, None, options.into(), true)
            .await
    }

    pub async fn validate_package(
        &self,
        package: &str,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/validate", package_path(package));
        self.send_json::<JsonValueEnvelope, ()>(Method::POST, &path, None, options, false)
            .await
    }

    pub async fn test_package(
        &self,
        package: &str,
        body: &CapabilitiesRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/test", package_path(package));
        self.send_json(Method::POST, &path, Some(body), options, false)
            .await
    }

    pub async fn export_package_identity(
        &self,
        package: &str,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/identity", package_path(package));
        self.send_json::<JsonValueEnvelope, ()>(Method::GET, &path, None, options, false)
            .await
    }

    pub async fn sign_package(
        &self,
        package: &str,
        body: &SignPackageRequest,
        options: CasCallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/sign", package_path(package));
        self.send_json(Method::POST, &path, Some(body), options.into(), true)
            .await
    }

    pub async fn verify_package_trust(
        &self,
        package: &str,
        body: &VerifyPackageTrustRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/verify-trust", package_path(package));
        self.send_json(Method::POST, &path, Some(body), options, false)
            .await
    }

    pub async fn list_evals(
        &self,
        package: &str,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/evals", package_path(package));
        self.send_json::<JsonValueEnvelope, ()>(Method::GET, &path, None, options, false)
            .await
    }

    pub async fn run_eval(
        &self,
        package: &str,
        body: &RunEvalRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/evals/run", package_path(package));
        self.send_json(Method::POST, &path, Some(body), options, false)
            .await
    }

    pub async fn render_contract(
        &self,
        package: &str,
        contract: &str,
        body: &CompileRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/render", contract_path(package, contract));
        self.send_json(Method::POST, &path, Some(body), options, false)
            .await
    }

    pub async fn diff_contract(
        &self,
        package: &str,
        contract: &str,
        body: &DiffContractRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/diff", contract_path(package, contract));
        self.send_json(Method::POST, &path, Some(body), options, false)
            .await
    }

    pub async fn explain_contract(
        &self,
        package: &str,
        contract: &str,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = format!("{}/explain", contract_path(package, contract));
        self.send_json::<JsonValueEnvelope, ()>(Method::GET, &path, None, options, false)
            .await
    }

    pub async fn migrate_legacy(
        &self,
        body: &MigrateLegacyRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        self.send_json(
            Method::POST,
            "/operations/v1/legacy/migrate",
            Some(body),
            options,
            false,
        )
        .await
    }

    pub async fn render_document(
        &self,
        body: &RenderDocumentRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        self.send_json(
            Method::POST,
            "/operations/v1/documents/render",
            Some(body),
            options,
            false,
        )
        .await
    }

    pub async fn inspect_document(
        &self,
        body: &InspectDocumentRequest,
        options: CallOptions,
    ) -> Result<TempliqxResponse<InspectDocumentEnvelope>, TempliqxError> {
        self.send_json(
            Method::POST,
            "/operations/v1/documents/inspect",
            Some(body),
            options,
            false,
        )
        .await
    }

    pub async fn list_workspace_artifacts(
        &self,
        package: &str,
        workspace: Option<&str>,
        prefix: Option<&str>,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = with_query(
            "/operations/v1/artifacts",
            &[
                ("package", Some(package)),
                ("workspace", workspace),
                ("prefix", prefix),
            ],
        );
        self.send_json::<JsonValueEnvelope, ()>(Method::GET, &path, None, options, false)
            .await
    }

    pub async fn read_artifact(
        &self,
        artifact: &str,
        package: &str,
        workspace: Option<&str>,
        options: CallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = with_query(
            &format!(
                "/operations/v1/artifacts/{}",
                encode_artifact_path(artifact)
            ),
            &[("package", Some(package)), ("workspace", workspace)],
        );
        self.send_json::<JsonValueEnvelope, ()>(Method::GET, &path, None, options, false)
            .await
    }

    pub async fn delete_workspace_artifact(
        &self,
        artifact: &str,
        package: &str,
        workspace: Option<&str>,
        options: CasCallOptions,
    ) -> Result<TempliqxResponse<JsonValueEnvelope>, TempliqxError> {
        let path = with_query(
            &format!(
                "/operations/v1/artifacts/{}",
                encode_artifact_path(artifact)
            ),
            &[("package", Some(package)), ("workspace", workspace)],
        );
        self.send_json::<JsonValueEnvelope, ()>(Method::DELETE, &path, None, options.into(), true)
            .await
    }

    async fn send_json<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        options: CallOptions,
        cas: bool,
    ) -> Result<TempliqxResponse<T>, TempliqxError> {
        let request_id = options.request_id.clone().unwrap_or_else(random_uuid_v4);
        let mut request = self.request(method, path, &options, cas, &request_id);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|source| TempliqxTransportError {
                request_id: request_id.clone(),
                source,
            })?;
        let status = response.status();
        let response_request_id = response_request_id(&response, &request_id);
        if !status.is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|source| TempliqxTransportError {
                    request_id: response_request_id.clone(),
                    source,
                })?;
            return Err(http_error(status, bytes, response_request_id).into());
        }
        let data = response
            .json()
            .await
            .map_err(|source| TempliqxTransportError {
                request_id: response_request_id.clone(),
                source,
            })?;
        Ok(TempliqxResponse {
            data,
            request_id: response_request_id,
        })
    }

    async fn send_text(
        &self,
        method: Method,
        path: &str,
        options: CallOptions,
        cas: bool,
    ) -> Result<TempliqxResponse<String>, TempliqxError> {
        let request_id = options.request_id.clone().unwrap_or_else(random_uuid_v4);
        let response = self
            .request(method, path, &options, cas, &request_id)
            .send()
            .await
            .map_err(|source| TempliqxTransportError {
                request_id: request_id.clone(),
                source,
            })?;
        let (status, response_request_id, bytes) = response_parts(response, &request_id).await?;
        if !status.is_success() {
            return Err(http_error(status, bytes, response_request_id).into());
        }
        Ok(TempliqxResponse {
            data: String::from_utf8_lossy(&bytes).into_owned(),
            request_id: response_request_id,
        })
    }

    async fn send_yaml<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &str,
        options: CallOptions,
        cas: bool,
    ) -> Result<TempliqxResponse<T>, TempliqxError> {
        let request_id = options.request_id.clone().unwrap_or_else(random_uuid_v4);
        let response = self
            .request(method, path, &options, cas, &request_id)
            .header(reqwest::header::CONTENT_TYPE, "application/yaml")
            .body(body.to_owned())
            .send()
            .await
            .map_err(|source| TempliqxTransportError {
                request_id: request_id.clone(),
                source,
            })?;
        let status = response.status();
        let response_request_id = response_request_id(&response, &request_id);
        if !status.is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|source| TempliqxTransportError {
                    request_id: response_request_id.clone(),
                    source,
                })?;
            return Err(http_error(status, bytes, response_request_id).into());
        }
        let data = response
            .json()
            .await
            .map_err(|source| TempliqxTransportError {
                request_id: response_request_id.clone(),
                source,
            })?;
        Ok(TempliqxResponse {
            data,
            request_id: response_request_id,
        })
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        options: &CallOptions,
        cas: bool,
        request_id: &str,
    ) -> reqwest::RequestBuilder {
        let mut request = self
            .http
            .request(method, format!("{}{}", self.base_url, path))
            .header("x-request-id", request_id)
            .timeout(options.timeout.unwrap_or(self.timeout));
        if cas && let Some(if_match) = &options.if_match {
            request = request.header(reqwest::header::IF_MATCH, if_match);
        }
        request
    }
}

async fn response_parts(
    response: reqwest::Response,
    fallback_request_id: &str,
) -> Result<(StatusCode, String, Vec<u8>), TempliqxError> {
    let status = response.status();
    let request_id = response_request_id(&response, fallback_request_id);
    let bytes = response
        .bytes()
        .await
        .map_err(|source| TempliqxTransportError {
            request_id: request_id.clone(),
            source,
        })?
        .to_vec();
    Ok((status, request_id, bytes))
}

fn response_request_id(response: &reqwest::Response, fallback: &str) -> String {
    response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or(fallback)
        .to_owned()
}

fn http_error(
    status: StatusCode,
    bytes: impl AsRef<[u8]>,
    request_id: String,
) -> TempliqxHttpError {
    let bytes = bytes.as_ref();
    let envelope = serde_json::from_slice(bytes).ok();
    let raw_body = envelope
        .is_none()
        .then(|| String::from_utf8_lossy(bytes).into_owned());
    TempliqxHttpError {
        status,
        envelope,
        raw_body,
        request_id,
    }
}

fn package_path(package: &str) -> String {
    format!("/operations/v1/packages/{}", encode_component(package))
}

fn contract_path(package: &str, contract: &str) -> String {
    format!(
        "{}/contracts/{}",
        package_path(package),
        encode_component(contract)
    )
}

fn with_query(path: &str, fields: &[(&str, Option<&str>)]) -> String {
    let query = fields
        .iter()
        .filter_map(|(key, value)| value.map(|value| format!("{key}={}", encode_component(value))))
        .collect::<Vec<_>>()
        .join("&");
    if query.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{query}")
    }
}

fn encode_artifact_path(value: &str) -> String {
    value
        .split('/')
        .map(encode_component)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            write!(&mut encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

fn random_uuid_v4() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = UUID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let high = RandomState::new().hash_one((nanos, counter, std::process::id()));
    let low = RandomState::new().hash_one((counter, nanos.rotate_left(47), std::process::id()));
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&high.to_be_bytes());
    bytes[8..].copy_from_slice(&low.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}
