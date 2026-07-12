//! Conformance-only HTTP transport for the mock runtime gateway.
//!
//! The adapter deliberately implements one request/response exchange. Retry
//! policy belongs to the host and is not hidden in this transport.

use std::{
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use templiqx_contracts::{AdapterDescriptor, ExecutionReceipt, ExecutionRequest, fingerprint};
use templiqx_ports::{PortError, RuntimeAdapter, RuntimeFailure, RuntimeFailureCode};

/// The conformance gateway's scenario execution endpoint.
pub const SCENARIO_ENDPOINT_PREFIX: &str = "/v1/scenarios/";

#[derive(Debug, Clone)]
pub struct HttpMockRuntime {
    endpoint: Endpoint,
    scenario_id: String,
    timeout: Duration,
    descriptor: AdapterDescriptor,
}

#[derive(Debug, Clone)]
struct Endpoint {
    host: String,
    port: u16,
}

impl HttpMockRuntime {
    /// Construct an adapter for an `http://host:port` gateway URL and scenario.
    ///
    /// HTTPS is intentionally unsupported: this is a local conformance
    /// transport, not a production gateway client.
    pub fn new(
        base_url: &str,
        scenario_id: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, PortError> {
        let endpoint = Endpoint::parse(base_url)?;
        let scenario_id = scenario_id.into();
        if scenario_id.is_empty() || scenario_id.contains('/') || scenario_id.contains('\\') {
            return Err(PortError::InvalidData(
                "mock runtime scenario id is invalid".into(),
            ));
        }
        Ok(Self {
            endpoint,
            scenario_id,
            timeout,
            descriptor: AdapterDescriptor {
                id: "templiqx-runtime-http-mock".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                capabilities: vec!["structured_output".into()],
            },
        })
    }

    /// Construct using the default five-second transport timeout.
    pub fn with_default_timeout(
        base_url: &str,
        scenario_id: impl Into<String>,
    ) -> Result<Self, PortError> {
        Self::new(base_url, scenario_id, Duration::from_secs(5))
    }

    fn failure(&self, code: RuntimeFailureCode, detail: impl Into<String>) -> PortError {
        self.failure_with_retry(code, detail, None)
    }

    fn failure_with_retry(
        &self,
        code: RuntimeFailureCode,
        detail: impl Into<String>,
        retry_after_ms: Option<u64>,
    ) -> PortError {
        let detail = detail.into();
        let fingerprint = fingerprint(&serde_json::json!({
            "adapter": self.descriptor.id,
            "code": code.as_str(),
            "detail": detail,
        }))
        .unwrap_or_else(|_| "http-mock-failure".into());
        RuntimeFailure {
            code,
            adapter_id: self.descriptor.id.clone(),
            adapter_version: self.descriptor.version.clone(),
            scenario_id: None,
            retry_after_ms,
            fingerprint,
            detail,
        }
        .into()
    }

    fn exchange(&self, body: &[u8]) -> Result<Vec<u8>, PortError> {
        let address = format!("{}:{}", self.endpoint.host, self.endpoint.port);
        let mut addresses = address
            .to_socket_addrs()
            .map_err(|error| self.map_io(error))?;
        let socket = addresses.next().ok_or_else(|| {
            self.failure(
                RuntimeFailureCode::Unavailable,
                "gateway address resolved to no endpoints",
            )
        })?;
        let mut stream = TcpStream::connect_timeout(&socket, self.timeout)
            .map_err(|error| self.map_io(error))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|error| self.map_io(error))?;

        let request = format!(
            "POST {SCENARIO_ENDPOINT_PREFIX}{}/execute HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.scenario_id,
            self.endpoint.host,
            body.len()
        );
        stream
            .write_all(request.as_bytes())
            .and_then(|_| stream.write_all(body))
            .map_err(|error| self.map_io(error))?;
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(|error| self.map_io(error))?;
        parse_response(&response, |code, detail, retry_after_ms| {
            self.failure_with_retry(code, detail, retry_after_ms)
        })
    }

    fn map_io(&self, error: io::Error) -> PortError {
        let code = if matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            RuntimeFailureCode::Timeout
        } else {
            RuntimeFailureCode::Unavailable
        };
        self.failure(code, error.kind().to_string())
    }
}

impl RuntimeAdapter for HttpMockRuntime {
    fn descriptor(&self) -> AdapterDescriptor {
        self.descriptor.clone()
    }

    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError> {
        let body = serde_json::to_vec(request).map_err(|error| {
            self.failure(
                RuntimeFailureCode::InvalidResponse,
                format!("encode request: {error}"),
            )
        })?;
        let response = self.exchange(&body)?;
        let envelope: ScenarioResponse = serde_json::from_slice(&response).map_err(|error| {
            self.failure(
                RuntimeFailureCode::InvalidResponse,
                format!("decode gateway response: {error}"),
            )
        })?;
        if envelope.api_version != "templiqx.mock/v1alpha1"
            || envelope.scenario_id != self.scenario_id
        {
            return Err(self.failure(
                RuntimeFailureCode::InvalidResponse,
                "gateway response identity does not match request",
            ));
        }
        match envelope.outcome {
            Outcome::Success {
                request_fingerprint,
                output_fingerprint,
                output_schema_valid,
            } => Ok(ExecutionReceipt {
                adapter: self.descriptor(),
                request_fingerprint,
                output_fingerprint,
                output: serde_json::Value::Null,
                output_schema_valid,
            }),
            Outcome::Failure {
                code,
                detail,
                retry_after_ms,
                failure_fingerprint,
            } => {
                let code = parse_failure_code(&code).ok_or_else(|| {
                    self.failure(
                        RuntimeFailureCode::InvalidResponse,
                        format!("unknown gateway failure code {code}"),
                    )
                })?;
                Err(RuntimeFailure {
                    code,
                    adapter_id: self.descriptor.id.clone(),
                    adapter_version: self.descriptor.version.clone(),
                    scenario_id: Some(self.scenario_id.clone()),
                    retry_after_ms,
                    fingerprint: failure_fingerprint,
                    detail,
                }
                .into())
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioResponse {
    api_version: String,
    scenario_id: String,
    #[allow(dead_code)]
    contract: String,
    #[allow(dead_code)]
    scenario_fingerprint: String,
    #[allow(dead_code)]
    attempts: usize,
    #[allow(dead_code)]
    elapsed_ms: u64,
    outcome: Outcome,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Outcome {
    Success {
        request_fingerprint: String,
        output_fingerprint: String,
        output_schema_valid: bool,
    },
    Failure {
        code: String,
        detail: String,
        retry_after_ms: Option<u64>,
        failure_fingerprint: String,
    },
}

fn parse_failure_code(code: &str) -> Option<RuntimeFailureCode> {
    [
        RuntimeFailureCode::Timeout,
        RuntimeFailureCode::RateLimited,
        RuntimeFailureCode::Unavailable,
        RuntimeFailureCode::InvalidResponse,
        RuntimeFailureCode::Permanent,
        RuntimeFailureCode::HostRetryExhausted,
    ]
    .into_iter()
    .find(|candidate| candidate.as_str() == code)
}

impl Endpoint {
    fn parse(base_url: &str) -> Result<Self, PortError> {
        let rest = base_url.strip_prefix("http://").ok_or_else(|| {
            PortError::InvalidData("mock runtime endpoint must use an http:// URL".into())
        })?;
        let authority = rest.split('/').next().unwrap_or_default();
        if authority.is_empty() || authority.contains('@') {
            return Err(PortError::InvalidData(
                "mock runtime endpoint has no valid authority".into(),
            ));
        }
        let (host, port) = authority.rsplit_once(':').ok_or_else(|| {
            PortError::InvalidData("mock runtime endpoint must include an explicit port".into())
        })?;
        let port = port.parse().map_err(|_| {
            PortError::InvalidData("mock runtime endpoint has an invalid port".into())
        })?;
        if host.is_empty() {
            return Err(PortError::InvalidData(
                "mock runtime endpoint has an empty host".into(),
            ));
        }
        Ok(Self {
            host: host.into(),
            port,
        })
    }
}

fn parse_response(
    response: &[u8],
    failure: impl Fn(RuntimeFailureCode, String, Option<u64>) -> PortError,
) -> Result<Vec<u8>, PortError> {
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| {
            failure(
                RuntimeFailureCode::InvalidResponse,
                "gateway response has no header terminator".into(),
                None,
            )
        })?;
    let (head, body) = response.split_at(separator + 4);
    let status = String::from_utf8_lossy(head)
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| {
            failure(
                RuntimeFailureCode::InvalidResponse,
                "gateway response has an invalid status line".into(),
                None,
            )
        })?;
    let retry_after_ms = normalized_retry_after_ms(head);
    let mapped = match status {
        408 | 504 => Some(RuntimeFailureCode::Timeout),
        429 => Some(RuntimeFailureCode::RateLimited),
        502 | 503 => Some(RuntimeFailureCode::Unavailable),
        _ => None,
    };
    if let Some(code) = mapped {
        return Err(failure(
            code,
            format!("mock gateway returned HTTP {status}"),
            (code == RuntimeFailureCode::RateLimited)
                .then_some(retry_after_ms)
                .flatten(),
        ));
    }
    if !(200..300).contains(&status) {
        return Err(failure(
            RuntimeFailureCode::InvalidResponse,
            format!("mock gateway returned HTTP {status}"),
            None,
        ));
    }
    if body.is_empty() {
        return Err(failure(
            RuntimeFailureCode::InvalidResponse,
            "gateway response body is empty".into(),
            None,
        ));
    }
    Ok(body.to_vec())
}

fn normalized_retry_after_ms(headers: &[u8]) -> Option<u64> {
    let text = String::from_utf8_lossy(headers);
    for line in text.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("x-retry-after-ms") {
            return value.parse().ok();
        }
        if name.eq_ignore_ascii_case("retry-after") {
            return value.parse::<u64>().ok()?.checked_mul(1_000);
        }
    }
    None
}

#[cfg(test)]
mod tests;
