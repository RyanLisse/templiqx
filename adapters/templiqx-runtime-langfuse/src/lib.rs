//! Host-owned HTTP `RuntimeAdapter` that executes a real OpenAI-compatible
//! chat completion and traces the exchange to Langfuse.
//!
//! This is a production adapter, not a conformance mock: it holds real
//! credentials and makes real network calls. It must never be wired into
//! `templiqx-application`, `templiqx-cli`, or `templiqx-mcp`'s default
//! composition (`scripts/check-boundaries.sh` only forbids the *mock*
//! adapters there today — keeping this one out is a host wiring decision,
//! construct it explicitly in host code that owns credentials).
//!
//! Tracing is best-effort: a Langfuse outage must not fail contract
//! execution, so ingestion failures are logged to stderr and swallowed.
//! Uses Langfuse's legacy batch ingestion endpoint (`/api/public/ingestion`);
//! Langfuse's own docs now point new integrations at the OTLP endpoint
//! instead — swap `emit_trace` for an OTLP exporter if/when that matters.

use std::{io::Read, time::Duration};

use serde::Deserialize;
use serde_json::{Value, json};
use templiqx_contracts::{AdapterDescriptor, ExecutionReceipt, ExecutionRequest, fingerprint};
use templiqx_ports::{PortError, RuntimeAdapter, RuntimeFailure, RuntimeFailureCode};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const MAX_MODEL_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// OpenAI-compatible base URL, e.g. `https://api.openai.com/v1`.
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct LangfuseConfig {
    /// e.g. `https://cloud.langfuse.com`.
    pub host: String,
    pub public_key: String,
    pub secret_key: String,
}

#[derive(Debug, Clone)]
pub struct LangfuseTracedRuntime {
    model: ModelConfig,
    langfuse: LangfuseConfig,
    descriptor: AdapterDescriptor,
}

impl LangfuseTracedRuntime {
    pub fn new(model: ModelConfig, langfuse: LangfuseConfig) -> Result<Self, PortError> {
        require_non_empty("model base_url", &model.base_url)?;
        require_safe_http_url("model base_url", &model.base_url)?;
        require_non_empty("model api_key", &model.api_key)?;
        require_non_empty("model id", &model.model)?;
        require_non_empty("langfuse host", &langfuse.host)?;
        require_safe_http_url("langfuse host", &langfuse.host)?;
        require_non_empty("langfuse public_key", &langfuse.public_key)?;
        require_non_empty("langfuse secret_key", &langfuse.secret_key)?;
        Ok(Self {
            model,
            langfuse,
            descriptor: AdapterDescriptor {
                id: "templiqx-runtime-langfuse".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                capabilities: vec!["structured_output".into(), "chat_completion".into()],
            },
        })
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
        let detail = self.redact(&detail.into());
        let fingerprint = fingerprint(&json!({
            "adapter": self.descriptor.id,
            "code": code.as_str(),
            "detail": detail,
        }))
        .unwrap_or_else(|_| "langfuse-runtime-failure".into());
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

    fn call_model(
        &self,
        request: &ExecutionRequest,
    ) -> Result<(String, Option<ChatUsage>), PortError> {
        let url = format!(
            "{}/chat/completions",
            self.model.base_url.trim_end_matches('/')
        );
        let response = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.model.api_key))
            .set("Content-Type", "application/json")
            .timeout(self.model.timeout)
            .send_json(json!({
                "model": self.model.model,
                "messages": request.interaction.messages,
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "templiqx_output",
                        "strict": true,
                        "schema": request.interaction.output_schema,
                    }
                }
            }));

        let response = response.map_err(|error| self.map_ureq_error(error))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_MODEL_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                self.failure(
                    RuntimeFailureCode::InvalidResponse,
                    format!("read model response: {error}"),
                )
            })?;
        if bytes.len() as u64 > MAX_MODEL_RESPONSE_BYTES {
            return Err(self.failure(
                RuntimeFailureCode::InvalidResponse,
                "model response exceeded 2097152 byte limit",
            ));
        }
        let parsed: ChatCompletionResponse = serde_json::from_slice(&bytes).map_err(|error| {
            self.failure(
                RuntimeFailureCode::InvalidResponse,
                format!("decode model response: {error}"),
            )
        })?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| {
                self.failure(
                    RuntimeFailureCode::InvalidResponse,
                    "model response had no choices",
                )
            })?;
        Ok((content, parsed.usage))
    }

    fn map_ureq_error(&self, error: ureq::Error) -> PortError {
        match error {
            ureq::Error::Status(status, response) => {
                let code = match status {
                    429 => RuntimeFailureCode::RateLimited,
                    500..=599 => RuntimeFailureCode::Unavailable,
                    _ => RuntimeFailureCode::Permanent,
                };
                let retry_after_ms = (status == 429)
                    .then(|| normalized_retry_after_ms(&response))
                    .flatten();
                self.failure_with_retry(
                    code,
                    format!("model gateway returned HTTP {status}"),
                    retry_after_ms,
                )
            }
            ureq::Error::Transport(transport) => {
                // ponytail: coarse timeout detection via message text — ureq's
                // Transport doesn't expose a typed TimedOut variant. Escalate
                // to a proper io::Error downcast if this misclassifies in practice.
                let detail = transport.to_string();
                let code = if detail.to_lowercase().contains("timed out") {
                    RuntimeFailureCode::Timeout
                } else {
                    RuntimeFailureCode::Unavailable
                };
                self.failure(code, detail)
            }
        }
    }

    fn redact(&self, detail: &str) -> String {
        [
            self.model.api_key.as_str(),
            self.langfuse.public_key.as_str(),
            self.langfuse.secret_key.as_str(),
        ]
        .into_iter()
        .filter(|secret| !secret.is_empty())
        .fold(detail.to_owned(), |safe, secret| {
            safe.replace(secret, "[REDACTED]")
        })
    }

    /// Best-effort Langfuse ingestion. Never fails the caller: a tracing
    /// outage must not take down contract execution.
    fn emit_trace(
        &self,
        request: &ExecutionRequest,
        output: &Value,
        usage: Option<ChatUsage>,
        trace_id: &str,
    ) {
        if self
            .try_emit_trace(request, output, usage, trace_id)
            .is_err()
        {
            // Do not include the transport error: URL user-info, query strings,
            // or provider response text may contain host-owned credentials.
            eprintln!("templiqx-runtime-langfuse: trace emission failed (best effort)");
        }
    }

    fn try_emit_trace(
        &self,
        request: &ExecutionRequest,
        output: &Value,
        usage: Option<ChatUsage>,
        trace_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
        let input = json!(request.interaction.messages);
        let usage_json = usage.map(|u| {
            json!({
                "input": u.prompt_tokens,
                "output": u.completion_tokens,
                "total": u.total_tokens,
            })
        });
        let generation_id = format!("{trace_id}-generation");
        let batch = json!({
            "batch": [
                {
                    "id": format!("{trace_id}-trace-create"),
                    "type": "trace-create",
                    "timestamp": now,
                    "body": {
                        "id": trace_id,
                        "name": request.interaction.contract_id,
                        "input": input,
                        "output": output,
                    },
                },
                {
                    "id": format!("{trace_id}-generation-create"),
                    "type": "generation-create",
                    "timestamp": now,
                    "body": {
                        "id": generation_id,
                        "traceId": trace_id,
                        "name": request.interaction.contract_id,
                        "model": self.model.model,
                        "input": input,
                        "output": output,
                        "usage": usage_json,
                    },
                },
            ],
        });

        let url = format!(
            "{}/api/public/ingestion",
            self.langfuse.host.trim_end_matches('/')
        );
        let auth = basic_auth(&self.langfuse.public_key, &self.langfuse.secret_key);
        ureq::post(&url)
            .set("Authorization", &auth)
            .set("Content-Type", "application/json")
            .timeout(self.model.timeout)
            .send_json(batch)?;
        Ok(())
    }
}

impl RuntimeAdapter for LangfuseTracedRuntime {
    fn descriptor(&self) -> AdapterDescriptor {
        self.descriptor.clone()
    }

    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError> {
        let request_fingerprint = fingerprint(request).map_err(|error| {
            self.failure(
                RuntimeFailureCode::InvalidResponse,
                format!("fingerprint request: {error}"),
            )
        })?;

        let (content, usage) = self.call_model(request)?;
        let output: Value = serde_json::from_str(&content).map_err(|error| {
            self.failure(
                RuntimeFailureCode::InvalidResponse,
                format!("model output was not valid JSON: {error}"),
            )
        })?;

        let output_schema_valid = jsonschema::validator_for(&request.interaction.output_schema)
            .map(|validator| validator.is_valid(&output))
            .unwrap_or(false);
        let output_fingerprint = fingerprint(&output).map_err(|error| {
            self.failure(
                RuntimeFailureCode::InvalidResponse,
                format!("fingerprint output: {error}"),
            )
        })?;

        self.emit_trace(request, &output, usage, &request_fingerprint);

        Ok(ExecutionReceipt {
            adapter: self.descriptor(),
            request_fingerprint,
            output_fingerprint,
            output,
            output_schema_valid,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageContent,
}

#[derive(Debug, Deserialize)]
struct ChatMessageContent {
    content: String,
}

#[derive(Debug, Deserialize, Clone, Copy)]
struct ChatUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

fn require_non_empty(field: &str, value: &str) -> Result<(), PortError> {
    if value.trim().is_empty() {
        return Err(PortError::InvalidData(format!("{field} must not be empty")));
    }
    Ok(())
}

fn require_safe_http_url(field: &str, value: &str) -> Result<(), PortError> {
    let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    else {
        return Err(PortError::InvalidData(format!(
            "{field} must use http:// or https://"
        )));
    };
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') || value.contains('?') || value.contains('#')
    {
        return Err(PortError::InvalidData(format!(
            "{field} must not contain credentials, query, or fragment"
        )));
    }
    Ok(())
}

fn normalized_retry_after_ms(response: &ureq::Response) -> Option<u64> {
    response
        .header("x-retry-after-ms")
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            response
                .header("retry-after")
                .and_then(|value| value.parse::<u64>().ok())
                .and_then(|seconds| seconds.checked_mul(1_000))
        })
}

fn basic_auth(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64_encode(format!("{username}:{password}").as_bytes())
    )
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        out.push(BASE64_ALPHABET[(b0 >> 2) as usize] as char);
        out.push(BASE64_ALPHABET[(((b0 & 0x03) << 4) | (b1.unwrap_or(0) >> 4)) as usize] as char);
        out.push(match b1 {
            Some(b1) => {
                BASE64_ALPHABET[(((b1 & 0x0f) << 2) | (b2.unwrap_or(0) >> 6)) as usize] as char
            }
            None => '=',
        });
        out.push(match b2 {
            Some(b2) => BASE64_ALPHABET[(b2 & 0x3f) as usize] as char,
            None => '=',
        });
    }
    out
}

#[cfg(test)]
mod tests;
