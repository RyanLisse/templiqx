//! Conformance-only deterministic runtime scenarios.
//!
//! This crate is an adapter test fixture. It parses checked-in mock scenario
//! contracts and exposes a virtual-clock runtime, but it deliberately does not
//! own auth, retrieval, workflow, tenant policy or approval policy.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
};
use templiqx_contracts::{
    AdapterDescriptor, ExecutionReceipt, ExecutionRequest, StreamEvent, fingerprint,
};
use templiqx_ports::{PortError, RuntimeAdapter, RuntimeFailure, RuntimeFailureCode};

pub const MOCK_API_VERSION: &str = "templiqx.mock/v1alpha1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioInventory {
    pub api_version: String,
    pub package: String,
    pub scenarios: Vec<ScenarioInventoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioInventoryEntry {
    pub id: String,
    pub manifest: String,
    pub expectation: ScenarioExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioExpectation {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema_valid: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_fingerprint: Option<String>,
    pub receipt_fingerprint: String,
}

pub fn load_inventory(
    path: impl AsRef<Path>,
    expected_package: &str,
) -> Result<ScenarioInventory, ScenarioError> {
    let path = path.as_ref();
    let inventory: ScenarioInventory =
        serde_json::from_slice(&std::fs::read(path).map_err(|source| ScenarioError::Io {
            path: path.display().to_string(),
            source,
        })?)?;
    if inventory.api_version != MOCK_API_VERSION {
        return Err(ScenarioError::Invalid {
            message: format!(
                "unsupported inventory api_version '{}'",
                inventory.api_version
            ),
        });
    }
    if inventory.package != expected_package {
        return Err(ScenarioError::Invalid {
            message: format!(
                "inventory package '{}' does not match '{}'",
                inventory.package, expected_package
            ),
        });
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let root = path.parent().unwrap_or(Path::new("."));
    for entry in &inventory.scenarios {
        if entry.id.trim().is_empty() || entry.manifest.trim().is_empty() {
            return Err(ScenarioError::Invalid {
                message: "inventory entries require non-empty id and manifest".into(),
            });
        }
        if !ids.insert(entry.id.clone()) {
            return Err(ScenarioError::Invalid {
                message: format!("duplicate inventory id '{}'", entry.id),
            });
        }
        if !paths.insert(entry.manifest.clone()) {
            return Err(ScenarioError::Invalid {
                message: format!("duplicate inventory manifest '{}'", entry.manifest),
            });
        }
        let relative = Path::new(&entry.manifest);
        if relative.is_absolute()
            || entry.manifest.contains('\\')
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(ScenarioError::Invalid {
                message: format!(
                    "inventory manifest path is not package-relative: {}",
                    entry.manifest
                ),
            });
        }
        let manifest_path = root
            .parent()
            .and_then(Path::parent)
            .unwrap_or(root)
            .join(&entry.manifest);
        if !manifest_path.is_file() {
            return Err(ScenarioError::Invalid {
                message: format!("inventory manifest is missing: {}", entry.manifest),
            });
        }
        let manifest = ScenarioManifest::load(&manifest_path)?;
        if manifest.id != entry.id {
            return Err(ScenarioError::Invalid {
                message: format!(
                    "inventory id '{}' does not match manifest id '{}'",
                    entry.id, manifest.id
                ),
            });
        }
        let expected_schema_valid = manifest
            .steps
            .iter()
            .find_map(|step| {
                (step.kind == ScenarioStepKind::RuntimeSuccess)
                    .then_some(step.output_schema_valid.unwrap_or(true))
            })
            .or_else(|| {
                manifest.events.as_ref().and_then(|events| {
                    events.iter().find_map(|event| match event {
                        ScenarioStreamEvent::Finish {
                            output_schema_valid,
                            ..
                        } => Some(*output_schema_valid),
                        _ => None,
                    })
                })
            });
        if entry.expectation.status != manifest.expected_status
            || entry.expectation.diagnostic_code != manifest.expected_failure
            || entry.expectation.output_schema_valid != expected_schema_valid
            || entry.expectation.output_fingerprint != manifest.expected_output_fingerprint
        {
            return Err(ScenarioError::Invalid {
                message: format!(
                    "inventory expectation for '{}' does not match its manifest",
                    entry.id
                ),
            });
        }
    }
    Ok(inventory)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioKind {
    HappyPath,
    Ambiguous,
    Missing,
    Invalid,
    Drafting,
    Failure,
    DocumentWarning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptPayloadPolicy {
    FingerprintsOnly,
    NoSuccessfulReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioManifest {
    pub api_version: String,
    pub id: String,
    pub contract: String,
    pub kind: ScenarioKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_output_fingerprint: Option<String>,
    #[serde(default)]
    pub expected_diagnostics: Vec<String>,
    #[serde(default)]
    pub expected_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_failure: Option<String>,
    pub receipt_payload_policy: ReceiptPayloadPolicy,
    #[serde(default)]
    pub steps: Vec<ScenarioStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<ScenarioStreamEvent>>,
    #[serde(default)]
    pub evidence: Vec<EvidenceExpectation>,
    #[serde(default)]
    pub document_expectation: DocumentExpectation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub golden_receipt_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentExpectation {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub unresolved_references: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceExpectation {
    pub document_id: String,
    pub fragment_id: String,
    pub quote_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScenarioStreamEvent {
    Start {
        id: String,
    },
    Delta {
        id: String,
        text: String,
    },
    Reasoning {
        id: String,
        text: String,
    },
    Usage {
        id: String,
        input_tokens: u64,
        output_tokens: u64,
    },
    End {
        id: String,
    },
    Finish {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Value>,
        #[serde(default)]
        output_schema_valid: bool,
    },
}

impl ScenarioStreamEvent {
    fn id(&self) -> &str {
        match self {
            Self::Start { id }
            | Self::Delta { id, .. }
            | Self::Reasoning { id, .. }
            | Self::Usage { id, .. }
            | Self::End { id }
            | Self::Finish { id, .. } => id,
        }
    }
}

impl ScenarioManifest {
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, ScenarioError> {
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn from_yaml_slice(bytes: &[u8]) -> Result<Self, ScenarioError> {
        let manifest: Self = serde_yaml_ng::from_slice(bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ScenarioError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| ScenarioError::Io {
            path: path.display().to_string(),
            source,
        })?;
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("yaml" | "yml") => Self::from_yaml_slice(&bytes),
            _ => Self::from_json_slice(&bytes),
        }
    }

    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.api_version != MOCK_API_VERSION {
            return Err(ScenarioError::Invalid {
                message: format!("unsupported mock api_version '{}'", self.api_version),
            });
        }
        if self.id.trim().is_empty() || self.contract.trim().is_empty() {
            return Err(ScenarioError::Invalid {
                message: "scenario id and contract must be non-empty".into(),
            });
        }
        if self.steps.is_empty() == self.events.is_none() {
            return Err(ScenarioError::Invalid {
                message: "scenario must contain exactly one non-empty steps or events sequence"
                    .into(),
            });
        }

        if let Some(events) = &self.events {
            validate_stream_events(events)?;
            return Ok(());
        }

        let mut ids = BTreeSet::new();
        let mut saw_request = false;
        let mut saw_terminal = false;

        for (index, step) in self.steps.iter().enumerate() {
            if step.id.trim().is_empty() {
                return Err(ScenarioError::Invalid {
                    message: "step id must be non-empty".into(),
                });
            }
            if !ids.insert(step.id.clone()) {
                return Err(ScenarioError::Invalid {
                    message: format!("duplicate event id '{}'", step.id),
                });
            }
            if saw_terminal {
                return Err(ScenarioError::Invalid {
                    message: format!("step '{}' appears after terminal runtime step", step.id),
                });
            }

            match step.kind {
                ScenarioStepKind::RequestReceived => {
                    if index != 0 {
                        return Err(ScenarioError::Invalid {
                            message: "request_received must be the first step".into(),
                        });
                    }
                    saw_request = true;
                }
                ScenarioStepKind::Delay => {
                    if !saw_request {
                        return Err(ScenarioError::Invalid {
                            message: "delay cannot occur before request_received".into(),
                        });
                    }
                    if step.delay_ms.unwrap_or_default() == 0 {
                        return Err(ScenarioError::Invalid {
                            message: format!("delay step '{}' must set delay_ms > 0", step.id),
                        });
                    }
                }
                ScenarioStepKind::RuntimeSuccess => {
                    if !saw_request {
                        return Err(ScenarioError::Invalid {
                            message: "runtime_success cannot occur before request_received".into(),
                        });
                    }
                    if step.failure.is_some() {
                        return Err(ScenarioError::Invalid {
                            message: format!(
                                "runtime_success step '{}' cannot include failure",
                                step.id
                            ),
                        });
                    }
                    saw_terminal = true;
                }
                ScenarioStepKind::RuntimeFailure => {
                    if !saw_request {
                        return Err(ScenarioError::Invalid {
                            message: "runtime_failure cannot occur before request_received".into(),
                        });
                    }
                    if step.failure.is_none() {
                        return Err(ScenarioError::Invalid {
                            message: format!(
                                "runtime_failure step '{}' must include failure",
                                step.id
                            ),
                        });
                    }
                    saw_terminal = true;
                }
            }
        }

        if !saw_terminal {
            return Err(ScenarioError::Invalid {
                message: "scenario must end in runtime_success or runtime_failure".into(),
            });
        }

        Ok(())
    }
}

fn validate_stream_events(events: &[ScenarioStreamEvent]) -> Result<(), ScenarioError> {
    if events.is_empty() {
        return Err(ScenarioError::Invalid {
            message: "stream event sequence cannot be empty".into(),
        });
    }
    let mut ids = BTreeSet::new();
    let mut started = false;
    let mut ended = false;
    let mut finished = false;
    for event in events {
        if event.id().trim().is_empty() {
            return Err(ScenarioError::Invalid {
                message: "stream event id must be non-empty".into(),
            });
        }
        if !ids.insert(event.id().to_owned()) {
            return Err(ScenarioError::Invalid {
                message: format!("duplicate event id '{}'", event.id()),
            });
        }
        match event {
            ScenarioStreamEvent::Start { .. } if started || ended => {
                return Err(ScenarioError::Invalid {
                    message: "stream start must be first and unique".into(),
                });
            }
            ScenarioStreamEvent::Start { .. } => started = true,
            ScenarioStreamEvent::Delta { text, .. }
            | ScenarioStreamEvent::Reasoning { text, .. }
                if !started || ended || text.is_empty() =>
            {
                return Err(ScenarioError::Invalid {
                    message: "delta/reasoning requires a started, non-empty stream before end"
                        .into(),
                });
            }
            ScenarioStreamEvent::Delta { .. } | ScenarioStreamEvent::Reasoning { .. } => {}
            ScenarioStreamEvent::Usage { .. } if !started || ended || finished => {
                return Err(ScenarioError::Invalid {
                    message: "usage requires an active stream".into(),
                });
            }
            ScenarioStreamEvent::Usage { .. } => {}
            ScenarioStreamEvent::End { .. } if !started || ended || finished => {
                return Err(ScenarioError::Invalid {
                    message: "end requires an active stream".into(),
                });
            }
            ScenarioStreamEvent::End { .. } => ended = true,
            ScenarioStreamEvent::Finish { .. } if !started || !ended || finished => {
                return Err(ScenarioError::Invalid {
                    message: "finish requires start followed by end".into(),
                });
            }
            ScenarioStreamEvent::Finish { .. } => finished = true,
        }
    }
    if !(started && ended && finished) {
        return Err(ScenarioError::Invalid {
            message: "stream must contain start, end and finish".into(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioStep {
    pub id: String,
    pub kind: ScenarioStepKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema_valid: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<RuntimeFailureSpec>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioStepKind {
    RequestReceived,
    Delay,
    RuntimeSuccess,
    RuntimeFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFailureSpec {
    pub code: RuntimeFailureName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeFailureName {
    #[serde(rename = "TQX_RUNTIME_TIMEOUT")]
    Timeout,
    #[serde(rename = "TQX_RUNTIME_RATE_LIMITED")]
    RateLimited,
    #[serde(rename = "TQX_RUNTIME_UNAVAILABLE")]
    Unavailable,
    #[serde(rename = "TQX_RUNTIME_INVALID_RESPONSE")]
    InvalidResponse,
    #[serde(rename = "TQX_RUNTIME_PERMANENT")]
    Permanent,
    #[serde(rename = "TQX_HOST_RETRY_EXHAUSTED")]
    HostRetryExhausted,
}

impl RuntimeFailureName {
    #[must_use]
    pub fn as_code(self) -> RuntimeFailureCode {
        match self {
            Self::Timeout => RuntimeFailureCode::Timeout,
            Self::RateLimited => RuntimeFailureCode::RateLimited,
            Self::Unavailable => RuntimeFailureCode::Unavailable,
            Self::InvalidResponse => RuntimeFailureCode::InvalidResponse,
            Self::Permanent => RuntimeFailureCode::Permanent,
            Self::HostRetryExhausted => RuntimeFailureCode::HostRetryExhausted,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("I/O error reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid JSON mock scenario: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid YAML mock scenario: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error("invalid mock scenario: {message}")]
    Invalid { message: String },
}

#[derive(Debug, Clone)]
pub enum ScriptedScenario {
    Success,
    Failure {
        id: String,
        code: RuntimeFailureCode,
        retry_after_ms: Option<u64>,
        detail: String,
    },
}

impl ScriptedScenario {
    #[must_use]
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Failure {
                code: RuntimeFailureCode::Timeout
                    | RuntimeFailureCode::RateLimited
                    | RuntimeFailureCode::Unavailable,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct VirtualClock {
    now_ms: u64,
}

impl VirtualClock {
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    pub fn advance(&mut self, delay_ms: u64) {
        self.now_ms = self.now_ms.saturating_add(delay_ms);
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeStats {
    pub attempts: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ScriptedRuntime {
    descriptor: AdapterDescriptor,
    state: Arc<Mutex<RuntimeState>>,
}

#[derive(Debug, Clone)]
struct RuntimeState {
    attempts: usize,
    clock: VirtualClock,
    scenarios: VecDeque<ScriptedScenario>,
    manifest_steps: Option<Vec<ScenarioStep>>,
    manifest_events: Option<Vec<ScenarioStreamEvent>>,
}

impl ScriptedRuntime {
    #[must_use]
    pub fn success() -> Self {
        Self::sequence([ScriptedScenario::Success])
    }

    #[must_use]
    pub fn failure(id: &str, code: RuntimeFailureCode) -> Self {
        Self::sequence([ScriptedScenario::Failure {
            id: id.into(),
            code,
            retry_after_ms: matches!(code, RuntimeFailureCode::RateLimited).then_some(1500),
            detail: format!("scripted runtime failure: {id}"),
        }])
    }

    #[must_use]
    pub fn sequence<const N: usize>(scenarios: [ScriptedScenario; N]) -> Self {
        Self::scripted(scenarios)
    }

    #[must_use]
    pub fn scripted(scenarios: impl IntoIterator<Item = ScriptedScenario>) -> Self {
        Self {
            descriptor: descriptor(),
            state: Arc::new(Mutex::new(RuntimeState {
                attempts: 0,
                clock: VirtualClock::default(),
                scenarios: scenarios.into_iter().collect(),
                manifest_steps: None,
                manifest_events: None,
            })),
        }
    }

    #[must_use]
    pub fn from_manifest(manifest: ScenarioManifest) -> Self {
        Self {
            descriptor: descriptor(),
            state: Arc::new(Mutex::new(RuntimeState {
                attempts: 0,
                clock: VirtualClock::default(),
                scenarios: VecDeque::new(),
                manifest_steps: (!manifest.steps.is_empty()).then_some(manifest.steps),
                manifest_events: manifest.events,
            })),
        }
    }

    #[must_use]
    pub fn stats(&self) -> RuntimeStats {
        let state = self.state.lock().expect("runtime state lock");
        RuntimeStats {
            attempts: state.attempts,
            elapsed_ms: state.clock.now_ms(),
        }
    }

    fn execute_manifest_steps(
        &self,
        state: &mut RuntimeState,
        request: &ExecutionRequest,
        steps: &[ScenarioStep],
    ) -> Result<ExecutionReceipt, PortError> {
        for step in steps {
            match step.kind {
                ScenarioStepKind::RequestReceived => {}
                ScenarioStepKind::Delay => state.clock.advance(step.delay_ms.unwrap_or_default()),
                ScenarioStepKind::RuntimeSuccess => {
                    let output = step
                        .output
                        .clone()
                        .or_else(|| request.fixture_output.clone())
                        .unwrap_or_default();
                    return self.success_receipt(
                        request,
                        output,
                        step.output_schema_valid.unwrap_or(true),
                    );
                }
                ScenarioStepKind::RuntimeFailure => {
                    let failure = step.failure.as_ref().expect("validated failure step");
                    return Err(self.runtime_failure(step.id.clone(), failure.clone()));
                }
            }
        }
        Err(PortError::InvalidData(
            "mock scenario has no terminal step".into(),
        ))
    }

    fn execute_manifest_events(
        &self,
        request: &ExecutionRequest,
        events: &[ScenarioStreamEvent],
    ) -> Result<ExecutionReceipt, PortError> {
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut input_tokens = 0;
        let mut output_tokens = 0;
        let mut finish = None;
        for event in events {
            match event {
                ScenarioStreamEvent::Delta { text: value, .. } => text.push_str(value),
                ScenarioStreamEvent::Reasoning { text: value, .. } => reasoning.push_str(value),
                ScenarioStreamEvent::Usage {
                    input_tokens: input,
                    output_tokens: output,
                    ..
                } => {
                    input_tokens += input;
                    output_tokens += output;
                }
                ScenarioStreamEvent::Finish {
                    output,
                    output_schema_valid,
                    ..
                } => {
                    finish = Some((output.clone(), *output_schema_valid));
                }
                ScenarioStreamEvent::Start { .. } | ScenarioStreamEvent::End { .. } => {}
            }
        }
        let (output, output_schema_valid) = finish.expect("validated stream has finish");
        let output = output
            .or_else(|| request.fixture_output.clone())
            .unwrap_or_else(|| {
                json!({
                    "text": text,
                    "reasoning": reasoning,
                    "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens}
                })
            });
        self.success_receipt(request, output, output_schema_valid)
    }

    fn execute_script(
        &self,
        request: &ExecutionRequest,
        scenario: ScriptedScenario,
    ) -> Result<ExecutionReceipt, PortError> {
        match scenario {
            ScriptedScenario::Success => self.success_receipt(
                request,
                request.fixture_output.clone().unwrap_or_default(),
                true,
            ),
            ScriptedScenario::Failure {
                id,
                code,
                retry_after_ms,
                detail,
            } => Err(self.runtime_failure(
                id,
                RuntimeFailureSpec {
                    code: runtime_failure_name(code),
                    retry_after_ms,
                    detail,
                },
            )),
        }
    }

    fn success_receipt(
        &self,
        request: &ExecutionRequest,
        output: Value,
        output_schema_valid: bool,
    ) -> Result<ExecutionReceipt, PortError> {
        Ok(ExecutionReceipt {
            adapter: self.descriptor(),
            request_fingerprint: fingerprint(&request.interaction)
                .map_err(|error| PortError::InvalidData(error.to_string()))?,
            output_fingerprint: fingerprint(&output)
                .map_err(|error| PortError::InvalidData(error.to_string()))?,
            output,
            output_schema_valid,
        })
    }

    fn runtime_failure(&self, id: String, spec: RuntimeFailureSpec) -> PortError {
        RuntimeFailure {
            code: spec.code.as_code(),
            adapter_id: self.descriptor.id.clone(),
            adapter_version: self.descriptor.version.clone(),
            scenario_id: Some(id.clone()),
            retry_after_ms: spec.retry_after_ms,
            fingerprint: failure_fingerprint(&id, &spec),
            detail: spec.detail,
        }
        .into()
    }
}

impl RuntimeAdapter for ScriptedRuntime {
    fn descriptor(&self) -> AdapterDescriptor {
        self.descriptor.clone()
    }

    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionReceipt, PortError> {
        let mut state = self.state.lock().expect("runtime state lock");
        state.attempts += 1;

        if let Some(steps) = state.manifest_steps.clone() {
            return self.execute_manifest_steps(&mut state, request, &steps);
        }
        if let Some(events) = state.manifest_events.clone() {
            return self.execute_manifest_events(request, &events);
        }

        let scenario = state
            .scenarios
            .pop_front()
            .unwrap_or(ScriptedScenario::Success);
        self.execute_script(request, scenario)
    }

    /// Deterministic streaming replay (KTD6): emit each scenario-fixture `Delta`
    /// as a contracts `StreamEvent::Delta`, then the terminal event. The terminal
    /// `Complete` carries the exact receipt `execute` produces (fingerprint
    /// parity), and mid-stream failures surface a `Failed` event with a stable
    /// diagnostic code before the `Err` returns.
    fn execute_streaming(
        &self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<ExecutionReceipt, PortError> {
        let replay = {
            let state = self.state.lock().expect("runtime state lock");
            state.manifest_events.clone()
        };
        if let Some(events) = replay {
            for event in &events {
                if let ScenarioStreamEvent::Delta { text, .. } = event {
                    sink(StreamEvent::Delta { text: text.clone() });
                }
            }
        }
        match self.execute(request) {
            Ok(receipt) => {
                sink(StreamEvent::Complete(receipt.clone()));
                Ok(receipt)
            }
            Err(error) => {
                sink(stream_failed_event(&error));
                Err(error)
            }
        }
    }
}

/// Map a terminal `PortError` to a stable-coded streaming `Failed` event,
/// mirroring the diagnostic codes the application surfaces for the same error.
fn stream_failed_event(error: &PortError) -> StreamEvent {
    let code = match error {
        PortError::RuntimeFailure { code, .. } => (*code).to_owned(),
        PortError::NotFound(_) => "TQX_NOT_FOUND".to_owned(),
        PortError::Conflict(_) => "TQX_CAS_CONFLICT".to_owned(),
        PortError::InvalidPath(_) => "TQX_PATH_INVALID".to_owned(),
        PortError::Unsupported(_) => "TQX_UNSUPPORTED".to_owned(),
        PortError::Io(_) => "TQX_IO".to_owned(),
        PortError::InvalidData(_) => "TQX_DATA_INVALID".to_owned(),
    };
    StreamEvent::Failed {
        code,
        message: error.to_string(),
    }
}

fn descriptor() -> AdapterDescriptor {
    AdapterDescriptor {
        id: "templiqx-scripted-runtime".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        capabilities: vec!["structured_output".into()],
    }
}

fn runtime_failure_name(code: RuntimeFailureCode) -> RuntimeFailureName {
    match code {
        RuntimeFailureCode::Timeout => RuntimeFailureName::Timeout,
        RuntimeFailureCode::RateLimited => RuntimeFailureName::RateLimited,
        RuntimeFailureCode::Unavailable => RuntimeFailureName::Unavailable,
        RuntimeFailureCode::InvalidResponse => RuntimeFailureName::InvalidResponse,
        RuntimeFailureCode::Permanent => RuntimeFailureName::Permanent,
        RuntimeFailureCode::HostRetryExhausted => RuntimeFailureName::HostRetryExhausted,
    }
}

fn failure_fingerprint(id: &str, spec: &RuntimeFailureSpec) -> String {
    fingerprint(&json!({
        "id": id,
        "code": spec.code,
        "retry_after_ms": spec.retry_after_ms,
    }))
    .expect("runtime failure fingerprint is serializable")
}

#[must_use]
pub fn scenario_fingerprint(manifest: &ScenarioManifest) -> String {
    let mut payload = BTreeMap::new();
    payload.insert("api_version", json!(manifest.api_version));
    payload.insert("id", json!(manifest.id));
    payload.insert("contract", json!(manifest.contract));
    payload.insert("kind", json!(manifest.kind));
    payload.insert(
        "expected_output_fingerprint",
        json!(manifest.expected_output_fingerprint),
    );
    payload.insert("expected_diagnostics", json!(manifest.expected_diagnostics));
    payload.insert("expected_status", json!(manifest.expected_status));
    payload.insert("expected_failure", json!(manifest.expected_failure));
    payload.insert(
        "receipt_payload_policy",
        json!(manifest.receipt_payload_policy),
    );
    payload.insert("steps", json!(manifest.steps));
    payload.insert("events", json!(manifest.events));
    payload.insert("evidence", json!(manifest.evidence));
    payload.insert("document_expectation", json!(manifest.document_expectation));
    fingerprint(&payload).expect("mock scenario fingerprint is serializable")
}

/// Fingerprint only the deterministic, payload-free execution receipt fields.
#[must_use]
pub fn success_receipt_fingerprint(receipt: &ExecutionReceipt) -> String {
    fingerprint(&json!({
        "outcome": "success",
        "request_fingerprint": receipt.request_fingerprint,
        "output_fingerprint": receipt.output_fingerprint,
        "output_schema_valid": receipt.output_schema_valid,
    }))
    .expect("receipt fingerprint is serializable")
}

/// Fingerprint only the deterministic, payload-free failure outcome fields.
#[must_use]
pub fn failure_receipt_fingerprint(
    code: &str,
    fingerprint_value: &str,
    retry_after_ms: Option<u64>,
) -> String {
    fingerprint(&json!({
        "outcome": "failure",
        "code": code,
        "failure_fingerprint": fingerprint_value,
        "retry_after_ms": retry_after_ms,
    }))
    .expect("failure receipt fingerprint is serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(events: Vec<ScenarioStreamEvent>) -> ScenarioManifest {
        ScenarioManifest {
            api_version: MOCK_API_VERSION.into(),
            id: "stream".into(),
            contract: "demo".into(),
            kind: ScenarioKind::HappyPath,
            input: None,
            expected_output: None,
            expected_output_fingerprint: None,
            expected_diagnostics: vec![],
            expected_status: "success".into(),
            expected_failure: None,
            receipt_payload_policy: ReceiptPayloadPolicy::FingerprintsOnly,
            steps: vec![],
            events: Some(events),
            evidence: vec![],
            document_expectation: DocumentExpectation::default(),
            golden_receipt_fingerprint: None,
        }
    }

    #[test]
    fn stream_lifecycle_is_strict_and_typed() {
        let valid = manifest(vec![
            ScenarioStreamEvent::Start { id: "s".into() },
            ScenarioStreamEvent::Reasoning {
                id: "r".into(),
                text: "plan".into(),
            },
            ScenarioStreamEvent::Delta {
                id: "d".into(),
                text: "hello".into(),
            },
            ScenarioStreamEvent::Usage {
                id: "u".into(),
                input_tokens: 2,
                output_tokens: 1,
            },
            ScenarioStreamEvent::End { id: "e".into() },
            ScenarioStreamEvent::Finish {
                id: "f".into(),
                output: None,
                output_schema_valid: true,
            },
        ]);
        assert!(valid.validate().is_ok());
        for events in [
            vec![],
            vec![ScenarioStreamEvent::Finish {
                id: "f".into(),
                output: None,
                output_schema_valid: true,
            }],
            vec![
                ScenarioStreamEvent::Start { id: "x".into() },
                ScenarioStreamEvent::Start { id: "x".into() },
            ],
            vec![
                ScenarioStreamEvent::Start { id: "s".into() },
                ScenarioStreamEvent::Finish {
                    id: "f".into(),
                    output: None,
                    output_schema_valid: true,
                },
            ],
        ] {
            assert!(manifest(events).validate().is_err());
        }

        for events in [
            vec![
                ScenarioStreamEvent::Start { id: "s".into() },
                ScenarioStreamEvent::End { id: "e".into() },
                ScenarioStreamEvent::Usage {
                    id: "u".into(),
                    input_tokens: 1,
                    output_tokens: 1,
                },
                ScenarioStreamEvent::Finish {
                    id: "f".into(),
                    output: None,
                    output_schema_valid: true,
                },
            ],
            vec![
                ScenarioStreamEvent::Start { id: "".into() },
                ScenarioStreamEvent::End { id: "e".into() },
                ScenarioStreamEvent::Finish {
                    id: "f".into(),
                    output: None,
                    output_schema_valid: true,
                },
            ],
        ] {
            assert!(manifest(events).validate().is_err());
        }

        let duplicate_request = ScenarioManifest {
            api_version: MOCK_API_VERSION.into(),
            id: "steps".into(),
            contract: "demo".into(),
            kind: ScenarioKind::HappyPath,
            input: None,
            expected_output: None,
            expected_output_fingerprint: None,
            expected_diagnostics: vec![],
            expected_status: "failure".into(),
            expected_failure: None,
            receipt_payload_policy: ReceiptPayloadPolicy::NoSuccessfulReceipt,
            steps: vec![
                ScenarioStep {
                    id: "request-1".into(),
                    kind: ScenarioStepKind::RequestReceived,
                    delay_ms: None,
                    output: None,
                    output_schema_valid: None,
                    failure: None,
                },
                ScenarioStep {
                    id: "request-2".into(),
                    kind: ScenarioStepKind::RequestReceived,
                    delay_ms: None,
                    output: None,
                    output_schema_valid: None,
                    failure: None,
                },
                ScenarioStep {
                    id: "done".into(),
                    kind: ScenarioStepKind::RuntimeFailure,
                    delay_ms: None,
                    output: None,
                    output_schema_valid: None,
                    failure: Some(RuntimeFailureSpec {
                        code: RuntimeFailureName::Permanent,
                        retry_after_ms: None,
                        detail: "bad".into(),
                    }),
                },
            ],
            events: None,
            evidence: vec![],
            document_expectation: DocumentExpectation::default(),
            golden_receipt_fingerprint: None,
        };
        assert!(duplicate_request.validate().is_err());
    }

    #[test]
    fn stream_events_aggregate_deterministically_through_sync_adapter() {
        let runtime = ScriptedRuntime::from_manifest(manifest(vec![
            ScenarioStreamEvent::Start { id: "s".into() },
            ScenarioStreamEvent::Delta {
                id: "d1".into(),
                text: "a".into(),
            },
            ScenarioStreamEvent::Delta {
                id: "d2".into(),
                text: "b".into(),
            },
            ScenarioStreamEvent::End { id: "e".into() },
            ScenarioStreamEvent::Finish {
                id: "f".into(),
                output: None,
                output_schema_valid: true,
            },
        ]));
        let request: ExecutionRequest = serde_json::from_value(json!({
            "interaction": {"compiler":"test","contract_id":"demo","contract_version":"1","messages":[],"output_schema":{},"required_capabilities":[],"target_capabilities":[],"runtime_policy":{},"extensions":{}},
            "fixture_output": null
        })).expect("test execution request");
        let first = runtime.execute(&request).expect("first receipt");
        let second = runtime.execute(&request).expect("second receipt");
        assert_eq!(first.output, second.output);
        assert_eq!(first.output["text"], "ab");
    }
}
