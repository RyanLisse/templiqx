//! Stable, serializable contracts shared by every Templiqx surface.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const API_VERSION: &str = "templiqx/v1alpha1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_pointer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(code: &str, message: impl Into<String>, pointer: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            file: None,
            json_pointer: Some(pointer.into()),
            span: None,
            help: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OperationEnvelope<T> {
    pub api_version: String,
    pub operation: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default)]
    pub fingerprints: BTreeMap<String, String>,
}

impl<T> OperationEnvelope<T> {
    #[must_use]
    pub fn new(operation: &str, result: Option<T>, diagnostics: Vec<Diagnostic>) -> Self {
        let ok = !diagnostics.iter().any(|d| d.severity == Severity::Error);
        Self {
            api_version: API_VERSION.into(),
            operation: operation.into(),
            ok,
            result,
            diagnostics,
            fingerprints: BTreeMap::new(),
        }
    }
    #[must_use]
    pub fn fingerprint(mut self, name: &str, value: impl Into<String>) -> Self {
        self.fingerprints.insert(name.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub api_version: String,
    pub package: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub contracts: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub evals: Vec<String>,
    #[serde(default)]
    pub migrations: Vec<String>,
    #[serde(default)]
    pub templates: Vec<String>,
    #[serde(default)]
    pub provenance: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    pub schema: Value,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub api_version: String,
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, FieldSpec>,
    #[serde(default)]
    pub context: BTreeMap<String, FieldSpec>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub messages: Vec<MessageTemplate>,
    pub output_schema: Value,
    #[serde(default)]
    pub runtime_policy: BTreeMap<String, Value>,
    #[serde(default)]
    pub extensions: BTreeMap<String, ExtensionSpec>,
    #[serde(default)]
    pub components: BTreeMap<String, ComponentDefinition>,
    #[serde(default)]
    pub provenance: BTreeMap<String, String>,
    #[serde(default)]
    pub evals: Vec<EvalFixture>,
}

/// A provider-specific option that remains portable because its value is
/// validated against an explicit bounded schema and guarded by a capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExtensionSpec {
    pub capability: String,
    pub schema: Value,
    pub value: Value,
}

/// Components authored before typed parameters were introduced remain
/// readable. The core only accepts a legacy component when all parameters can
/// be inferred safely; new components should use the typed form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ComponentDefinition {
    Typed(TypedComponent),
    Legacy(Vec<Node>),
}

impl ComponentDefinition {
    #[must_use]
    pub fn content(&self) -> &[Node] {
        match self {
            Self::Typed(component) => &component.content,
            Self::Legacy(content) => content,
        }
    }

    #[must_use]
    pub fn parameters(&self) -> Option<&BTreeMap<String, FieldSpec>> {
        match self {
            Self::Typed(component) => Some(&component.parameters),
            Self::Legacy(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TypedComponent {
    #[serde(default)]
    pub parameters: BTreeMap<String, FieldSpec>,
    pub content: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MessageTemplate {
    pub role: Role,
    pub content: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    Developer,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Node {
    Text {
        value: String,
    },
    Interpolate {
        expression: Expr,
        #[serde(default)]
        filters: Vec<Filter>,
    },
    When {
        condition: Expr,
        then: Vec<Node>,
        #[serde(default, rename = "else")]
        otherwise: Vec<Node>,
    },
    ForEach {
        collection: Expr,
        item: String,
        body: Vec<Node>,
        #[serde(default)]
        separator: String,
    },
    Component {
        name: String,
        #[serde(default)]
        with: BTreeMap<String, Expr>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expr {
    Ref { path: String },
    Literal { value: Value },
    Equals { left: Box<Expr>, right: Box<Expr> },
    Not { value: Box<Expr> },
    And { values: Vec<Expr> },
    Or { values: Vec<Expr> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    Trim,
    Lower,
    Upper,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvalFixture {
    pub id: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
    pub fake_output: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CompiledInteraction {
    pub compiler: String,
    pub contract_id: String,
    pub contract_version: String,
    pub messages: Vec<CompiledMessage>,
    pub output_schema: Value,
    pub required_capabilities: Vec<String>,
    pub target_capabilities: Vec<String>,
    pub runtime_policy: BTreeMap<String, Value>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CompiledMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AdapterDescriptor {
    pub id: String,
    pub version: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionRequest {
    pub interaction: CompiledInteraction,
    pub fixture_output: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReceipt {
    pub adapter: AdapterDescriptor,
    pub request_fingerprint: String,
    pub output_fingerprint: String,
    pub output: Value,
    pub output_schema_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContractSummary {
    pub package: String,
    pub id: String,
    pub version: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContractDiff {
    pub equal: bool,
    pub left_fingerprint: String,
    pub right_fingerprint: String,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Explanation {
    pub contract_id: String,
    pub summary: String,
    pub inputs: Vec<String>,
    pub context: Vec<String>,
    pub capabilities: Vec<String>,
    pub component_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TestReport {
    pub package: String,
    pub passed: usize,
    pub failed: usize,
    pub cases: Vec<TestCaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TestCaseResult {
    pub contract_id: String,
    pub fixture_id: String,
    pub passed: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub artifact_fingerprint: Option<String>,
}

/// Canonical semantic JSON recursively orders object keys before serialization.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    fn order(v: Value) -> Value {
        match v {
            Value::Object(map) => {
                Value::Object(map.into_iter().map(|(k, v)| (k, order(v))).collect())
            }
            Value::Array(xs) => Value::Array(xs.into_iter().map(order).collect()),
            other => other,
        }
    }
    serde_json::to_vec(&order(serde_json::to_value(value)?))
}

pub fn fingerprint<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    Ok(hex::encode(Sha256::digest(canonical_json(value)?)))
}

/// SHA-256 content identity for exact artifact bytes.
#[must_use]
pub fn fingerprint_bytes(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_ignores_object_key_order() {
        let a: Value = serde_json::from_str(r#"{"b":2,"a":1}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
        assert_eq!(fingerprint(&a).unwrap(), fingerprint(&b).unwrap());
        assert_eq!(
            fingerprint(&a).unwrap(),
            "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777"
        );
    }
}
