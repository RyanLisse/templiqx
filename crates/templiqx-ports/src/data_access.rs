use crate::PortError;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct AuthorizedQueryRequest {
    pub actor: String,
    pub entity: String,
    pub fields: Vec<String>,
    pub filter: Value,
}

pub trait DataIntrospectPort: Send + Sync {
    /// Describes field names and types without returning entity rows.
    fn describe_schema(&self, actor: &str) -> Result<Value, PortError>;
}

pub trait AuthorizedQueryPort: Send + Sync {
    fn query(&self, request: &AuthorizedQueryRequest) -> Result<Value, PortError>;
}
