//! Thin async client for the Templiqx Operations HTTP API.

pub mod client;
pub mod compat;
pub mod errors;
#[allow(clippy::all, clippy::pedantic)]
pub mod generated;

pub use client::{CallOptions, CasCallOptions, Client, ClientOptions, TempliqxResponse};
pub use compat::{COMPATIBILITY, Compatibility};
pub use errors::{TempliqxError, TempliqxHttpError, TempliqxTransportError};
