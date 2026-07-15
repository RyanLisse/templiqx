//! Transport-only error mapping.

use crate::generated::OperationEnvelopeBase;

/// A request failed before an HTTP response was received.
#[derive(Debug, thiserror::Error)]
#[error("Templiqx request {request_id} failed before receiving an HTTP response")]
pub struct TempliqxTransportError {
    pub request_id: String,
    #[source]
    pub source: reqwest::Error,
}

impl TempliqxTransportError {
    /// Whether the configured request timeout elapsed.
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        self.source.is_timeout()
    }
}

/// A non-success HTTP response.
#[derive(Debug, thiserror::Error)]
#[error("Templiqx request {request_id} failed with HTTP {status}")]
pub struct TempliqxHttpError {
    pub status: reqwest::StatusCode,
    pub envelope: Option<OperationEnvelopeBase>,
    pub raw_body: Option<String>,
    pub request_id: String,
}

/// Errors produced by the HTTP transport.
#[derive(Debug, thiserror::Error)]
pub enum TempliqxError {
    #[error(transparent)]
    Transport(#[from] TempliqxTransportError),
    #[error(transparent)]
    Http(#[from] TempliqxHttpError),
}
