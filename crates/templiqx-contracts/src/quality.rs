//! Portable DTOs and canonical identities for quality-proposal assessment.
//!
//! This module intentionally contains no provider or optimizer concepts. Profile
//! fingerprints are opaque host-attested claims; Templiqx validates their shape
//! and consistency but does not authenticate them.

use crate::{Diagnostic, EvalFixture, PackageIdentity, canonical_json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const QUALITY_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub const QUALITY_MAX_CANDIDATES: usize = 32;
pub const QUALITY_MAX_CANDIDATE_SOURCE_BYTES: usize = 512 * 1024;
pub const QUALITY_MAX_FIXTURES: usize = 512;
pub const QUALITY_MAX_REPLICATES: u16 = 20;
pub const QUALITY_MAX_TRIALS_PER_CANDIDATE: usize = 10_240;
pub const QUALITY_MAX_SCORERS: usize = 16;
pub const QUALITY_MAX_OBJECTIVES: usize = 16;
pub const QUALITY_MAX_OBSERVATIONS_PER_TRIAL: usize = 16;
pub const QUALITY_MAX_ID_BYTES: usize = 128;
pub const QUALITY_MAX_DIAGNOSTICS: usize = 256;
/// Largest integer that is losslessly representable by every public SDK,
/// including JavaScript/TypeScript `number`.
pub const QUALITY_MAX_PUBLIC_INTEGER: u64 = 9_007_199_254_740_991;

pub const TQX_QUALITY_BINDING_MISMATCH: &str = "TQX_QUALITY_BINDING_MISMATCH";
pub const TQX_QUALITY_POLICY_INVALID: &str = "TQX_QUALITY_POLICY_INVALID";
pub const TQX_QUALITY_EVIDENCE_INVALID: &str = "TQX_QUALITY_EVIDENCE_INVALID";
pub const TQX_QUALITY_METRIC_MISSING: &str = "TQX_QUALITY_METRIC_MISSING";
pub const TQX_QUALITY_METRIC_UNIT_MISMATCH: &str = "TQX_QUALITY_METRIC_UNIT_MISMATCH";
pub const TQX_QUALITY_INSUFFICIENT_COVERAGE: &str = "TQX_QUALITY_INSUFFICIENT_COVERAGE";
pub const TQX_QUALITY_INFRASTRUCTURE_BUDGET: &str = "TQX_QUALITY_INFRASTRUCTURE_BUDGET";
pub const TQX_QUALITY_GATE_FAILED: &str = "TQX_QUALITY_GATE_FAILED";
pub const TQX_QUALITY_CANDIDATE_INVALID: &str = "TQX_QUALITY_CANDIDATE_INVALID";
pub const TQX_QUALITY_BASE_STALE: &str = "TQX_QUALITY_BASE_STALE";
pub const TQX_QUALITY_LIMIT_EXCEEDED: &str = "TQX_QUALITY_LIMIT_EXCEEDED";
pub const TQX_QUALITY_DIAGNOSTICS_TRUNCATED: &str = "TQX_QUALITY_DIAGNOSTICS_TRUNCATED";

const PACKAGE_DOMAIN: &[u8] = b"templiqx-package-identity/v1\0";
const FIXTURE_DOMAIN: &[u8] = b"templiqx-quality-fixtures/v1\0";
const POLICY_DOMAIN: &[u8] = b"templiqx-quality-policy/v1\0";
const REQUEST_DOMAIN: &[u8] = b"templiqx-quality-request/v1\0";
const REPORT_DOMAIN: &[u8] = b"templiqx-quality-report/v1\0";

#[derive(Debug)]
pub enum QualityFingerprintError {
    DuplicateFixtureId(String),
    Serialization(serde_json::Error),
}

impl std::fmt::Display for QualityFingerprintError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateFixtureId(id) => write!(formatter, "duplicate fixture id: {id}"),
            Self::Serialization(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for QualityFingerprintError {}

impl From<serde_json::Error> for QualityFingerprintError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    RatioPpm,
    Milliseconds,
    TokenCount,
    CurrencyMicrounits,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MetricAggregation {
    BinaryRatioPpm,
    Mean,
    Sum,
    P95NearestRank,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveDirection {
    Maximize,
    Minimize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EligibilityComparator {
    Gte,
    Lte,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Prompt,
    Completion,
    Total,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct BinaryScorer {
    pub id: String,
    pub metric_id: String,
    pub claimed_scorer_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct QualityObjective {
    pub id: String,
    pub metric_id: String,
    pub unit: MetricUnit,
    pub aggregation: MetricAggregation,
    pub direction: ObjectiveDirection,
    pub claimed_measurement_profile_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_kind: Option<TokenKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct EligibilityRule {
    pub id: String,
    pub metric_id: String,
    pub comparator: EligibilityComparator,
    pub unit: MetricUnit,
    pub threshold: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QualityPolicy {
    pub id: String,
    pub replicates_per_fixture: u16,
    pub minimum_semantic_cases: u64,
    pub maximum_infrastructure_failure_ppm: u64,
    pub claimed_evaluator_profile_fingerprint: String,
    pub claimed_model_profile_fingerprint: String,
    pub binary_scorers: Vec<BinaryScorer>,
    pub objectives: Vec<QualityObjective>,
    pub eligibility_rules: Vec<EligibilityRule>,
}

impl QualityPolicy {
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.binary_scorers.sort();
        normalized.objectives.sort();
        normalized.eligibility_rules.sort();
        normalized
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QualityProposalRequest {
    pub package: String,
    pub contract_id: String,
    pub expected_package_fingerprint: String,
    pub expected_base_contract_fingerprint: String,
    pub expected_fixture_set_fingerprint: String,
    pub policy: QualityPolicy,
    pub candidates: Vec<QualityCandidateSubmission>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QualityCandidateSubmission {
    pub candidate_source: String,
    pub synthetic_or_sanitized_data_attestation: bool,
    pub evidence: CandidateEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CandidateEvidence {
    pub claimed_package_fingerprint: String,
    pub claimed_base_contract_fingerprint: String,
    pub claimed_fixture_set_fingerprint: String,
    pub claimed_candidate_contract_fingerprint: String,
    pub claimed_quality_policy_fingerprint: String,
    pub claimed_evaluator_profile_fingerprint: String,
    pub claimed_model_profile_fingerprint: String,
    pub claimed_scorer_fingerprints: BTreeMap<String, String>,
    pub claimed_measurement_profile_fingerprints: BTreeMap<String, String>,
    pub trials: Vec<TrialEvidence>,
}

impl CandidateEvidence {
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        for trial in &mut normalized.trials {
            trial.passed_scorers.sort();
            trial.failed_scorers.sort();
            trial.observations.sort();
        }
        normalized.trials.sort();
        normalized
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct TrialEvidence {
    pub fixture_id: String,
    pub replicate_index: u16,
    pub provider_attempt_count: u32,
    pub outcome: TrialOutcome,
    #[serde(default)]
    pub passed_scorers: Vec<String>,
    #[serde(default)]
    pub failed_scorers: Vec<String>,
    pub observations: Vec<MetricObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrialOutcome {
    Scored,
    CandidateQualityFailure {
        reason: CandidateQualityFailureReason,
    },
    InfrastructureFailure {
        reason: InfrastructureFailureReason,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CandidateQualityFailureReason {
    Schema,
    Assertion,
    InvalidOutput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureFailureReason {
    Transport,
    Timeout,
    RateLimit,
    ProviderUnavailable,
    ProviderInternal,
    Cancellation,
    Budget,
    EvaluatorInfrastructure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct MetricObservation {
    pub metric_id: String,
    pub unit: MetricUnit,
    pub value: u64,
    pub claimed_measurement_profile_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_kind: Option<TokenKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ComputedQualityIdentities {
    pub package_fingerprint: String,
    pub base_contract_fingerprint: String,
    pub fixture_set_fingerprint: String,
    pub quality_policy_fingerprint: String,
    pub request_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClaimedQualityIdentities {
    /// The host-attested candidate identity. This remains a claim even when
    /// candidate parsing fails and no computed identity is available.
    pub claimed_candidate_contract_fingerprint: String,
    pub claimed_evaluator_profile_fingerprint: String,
    pub claimed_model_profile_fingerprint: String,
    pub claimed_scorer_fingerprints: BTreeMap<String, String>,
    pub claimed_measurement_profile_fingerprints: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetricAggregate {
    pub metric_id: String,
    pub unit: MetricUnit,
    pub aggregation: MetricAggregation,
    pub direction: ObjectiveDirection,
    pub value: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EligibilityGate {
    pub rule_id: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<u64>,
    pub comparator: EligibilityComparator,
    pub threshold: u64,
    pub unit: MetricUnit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EligibilityAssessment {
    pub eligible: bool,
    pub total_trial_count: u64,
    pub semantic_trial_count: u64,
    pub infrastructure_trial_count: u64,
    pub semantic_coverage_ppm: u64,
    pub infrastructure_failure_ppm: u64,
    pub gates: Vec<EligibilityGate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QualityTrialSummary {
    pub fixture_id: String,
    pub replicate_index: u16,
    pub provider_attempt_count: u32,
    pub outcome: TrialOutcome,
    pub passed_scorers: Vec<String>,
    pub failed_scorers: Vec<String>,
    pub observations: Vec<MetricObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CandidateAssessment {
    /// Computed from the parsed contract. `None` means the source could not be
    /// parsed/validated; a host claim is never promoted to computed identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_fingerprint: Option<String>,
    /// Host-attested identities are returned only when every reported claim is
    /// syntactically valid and matches the validated protocol profile. Invalid
    /// evidence is diagnosed without reflecting attacker-controlled values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_identities: Option<ClaimedQualityIdentities>,
    pub eligibility: EligibilityAssessment,
    pub aggregates: Vec<MetricAggregate>,
    pub trial_summaries: Vec<QualityTrialSummary>,
    pub proposal_change_paths: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ParetoFront {
    pub rank: u32,
    pub candidate_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QualityProposalReportPayload {
    pub computed_identities: ComputedQualityIdentities,
    pub candidate_assessments: Vec<CandidateAssessment>,
    pub pareto_fronts: Vec<ParetoFront>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QualityProposalReport {
    pub computed_identities: ComputedQualityIdentities,
    pub candidate_assessments: Vec<CandidateAssessment>,
    pub pareto_fronts: Vec<ParetoFront>,
    pub report_fingerprint: String,
}

impl QualityProposalReport {
    #[must_use]
    pub fn payload(&self) -> QualityProposalReportPayload {
        QualityProposalReportPayload {
            computed_identities: self.computed_identities.clone(),
            candidate_assessments: self.candidate_assessments.clone(),
            pareto_fronts: self.pareto_fronts.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QualityCandidateFingerprintPayload {
    /// Computed semantic identity, absent for parse-invalid proposals.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_contract_fingerprint: Option<String>,
    pub synthetic_or_sanitized_data_attestation: bool,
    pub evidence: CandidateEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QualityRequestFingerprintPayload {
    pub package: String,
    pub contract_id: String,
    pub expected_package_fingerprint: String,
    pub expected_base_contract_fingerprint: String,
    pub expected_fixture_set_fingerprint: String,
    pub policy: QualityPolicy,
    pub candidates: Vec<QualityCandidateFingerprintPayload>,
}

impl QualityRequestFingerprintPayload {
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.policy = normalized.policy.normalized();
        for candidate in &mut normalized.candidates {
            candidate.evidence = candidate.evidence.normalized();
        }
        normalized.candidates.sort_by(|a, b| {
            a.candidate_contract_fingerprint
                .as_ref()
                .unwrap_or(&a.evidence.claimed_candidate_contract_fingerprint)
                .cmp(
                    b.candidate_contract_fingerprint
                        .as_ref()
                        .unwrap_or(&b.evidence.claimed_candidate_contract_fingerprint),
                )
                .then_with(|| {
                    a.synthetic_or_sanitized_data_attestation
                        .cmp(&b.synthetic_or_sanitized_data_attestation)
                })
                .then_with(|| {
                    canonical_json(&a.evidence)
                        .expect("quality evidence serialization is infallible")
                        .cmp(
                            &canonical_json(&b.evidence)
                                .expect("quality evidence serialization is infallible"),
                        )
                })
        });
        normalized
    }
}

fn domain_fingerprint<T: Serialize>(domain: &[u8], value: &T) -> Result<String, serde_json::Error> {
    let canonical = canonical_json(value)?;
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(canonical);
    Ok(hex::encode(digest.finalize()))
}

pub fn quality_package_fingerprint(
    identity: &PackageIdentity,
) -> Result<String, serde_json::Error> {
    domain_fingerprint(PACKAGE_DOMAIN, identity)
}

pub fn quality_fixture_set_fingerprint(
    fixtures: &[EvalFixture],
) -> Result<String, QualityFingerprintError> {
    let mut normalized = fixtures.to_vec();
    normalized.sort_by(|a, b| a.id.cmp(&b.id));
    let mut ids = BTreeSet::new();
    for fixture in &normalized {
        if !ids.insert(&fixture.id) {
            return Err(QualityFingerprintError::DuplicateFixtureId(
                fixture.id.clone(),
            ));
        }
    }
    Ok(domain_fingerprint(FIXTURE_DOMAIN, &normalized)?)
}

pub fn quality_policy_fingerprint(policy: &QualityPolicy) -> Result<String, serde_json::Error> {
    domain_fingerprint(POLICY_DOMAIN, &policy.normalized())
}

pub fn quality_request_fingerprint(
    payload: &QualityRequestFingerprintPayload,
) -> Result<String, serde_json::Error> {
    domain_fingerprint(REQUEST_DOMAIN, &payload.normalized())
}

pub fn quality_report_fingerprint(
    payload: &QualityProposalReportPayload,
) -> Result<String, serde_json::Error> {
    domain_fingerprint(REPORT_DOMAIN, payload)
}
