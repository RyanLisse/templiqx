//! Pure validation, aggregation, eligibility, and Pareto semantics for quality
//! proposal evidence.

use std::collections::{BTreeMap, BTreeSet};

use templiqx_contracts::{
    BinaryScorer, CandidateAssessment, CandidateEvidence, ClaimedQualityIdentities, Diagnostic,
    EligibilityAssessment, EligibilityComparator, EligibilityGate, EligibilityRule,
    MetricAggregate, MetricAggregation, MetricObservation, MetricUnit, ObjectiveDirection,
    ParetoFront, QUALITY_MAX_DIAGNOSTICS, QUALITY_MAX_FIXTURES, QUALITY_MAX_ID_BYTES,
    QUALITY_MAX_OBJECTIVES, QUALITY_MAX_OBSERVATIONS_PER_TRIAL, QUALITY_MAX_PUBLIC_INTEGER,
    QUALITY_MAX_REPLICATES, QUALITY_MAX_SCORERS, QUALITY_MAX_TRIALS_PER_CANDIDATE,
    QualityObjective, QualityPolicy, QualityTrialSummary, Severity, TQX_QUALITY_BINDING_MISMATCH,
    TQX_QUALITY_CANDIDATE_INVALID, TQX_QUALITY_DIAGNOSTICS_TRUNCATED, TQX_QUALITY_EVIDENCE_INVALID,
    TQX_QUALITY_GATE_FAILED, TQX_QUALITY_INFRASTRUCTURE_BUDGET, TQX_QUALITY_INSUFFICIENT_COVERAGE,
    TQX_QUALITY_LIMIT_EXCEEDED, TQX_QUALITY_METRIC_MISSING, TQX_QUALITY_METRIC_UNIT_MISMATCH,
    TQX_QUALITY_POLICY_INVALID, TrialEvidence, TrialOutcome,
};

const PPM: u128 = 1_000_000;
pub const ISO_4217_SOURCE: &str =
    "https://www.six-group.com/en/products-services/financial-information/data-standards.html";
pub const ISO_4217_SNAPSHOT_DATE: &str = "2026-07-22";
pub const ISO_4217_CODE_COUNT: usize = 179;
pub const ISO_4217_CODES_SHA256: &str =
    "4c64388da43ddb82dbf818d7f68a5f8511da6a46bf2ef318d28d9e36eea0a8a7";
pub const ISO_4217_CODES: &[&str] = &[
    "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT",
    "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BOV", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD",
    "CAD", "CDF", "CHE", "CHF", "CHW", "CLF", "CLP", "CNY", "COP", "COU", "CRC", "CUP", "CVE",
    "CZK", "DJF", "DKK", "DOP", "DZD", "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL",
    "GHS", "GIP", "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HTG", "HUF", "IDR", "ILS", "INR",
    "IQD", "IRR", "ISK", "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRW", "KWD",
    "KYD", "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD", "MMK",
    "MNT", "MOP", "MRU", "MUR", "MVR", "MWK", "MXN", "MXV", "MYR", "MZN", "NAD", "NGN", "NIO",
    "NOK", "NPR", "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN", "PYG", "QAR", "RON",
    "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SDG", "SEK", "SGD", "SHP", "SLE", "SOS", "SRD",
    "SSP", "STN", "SVC", "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD",
    "TZS", "UAH", "UGX", "USD", "USN", "UYI", "UYU", "UYW", "UZS", "VED", "VES", "VND", "VUV",
    "WST", "XAF", "XAG", "XAU", "XBA", "XBB", "XBC", "XBD", "XCD", "XCG", "XDR", "XOF", "XPD",
    "XPF", "XPT", "XSU", "XTS", "XUA", "XXX", "YER", "ZAR", "ZMW", "ZWG",
];

const PROPOSAL_CHANGE_PATHS: [&str; 14] = [
    "/api_version",
    "/id",
    "/version",
    "/description",
    "/inputs",
    "/context",
    "/capabilities",
    "/messages",
    "/output_schema",
    "/runtime_policy",
    "/extensions",
    "/components",
    "/provenance",
    "/evals",
];

/// Policy that passed all shape, comparability, and non-vacuous safety checks.
#[derive(Debug, Clone)]
pub struct ValidatedQualityPolicy {
    policy: QualityPolicy,
    scorers: BTreeMap<String, BinaryScorer>,
    objectives: BTreeMap<String, QualityObjective>,
}

impl ValidatedQualityPolicy {
    #[must_use]
    pub fn policy(&self) -> &QualityPolicy {
        &self.policy
    }
}

/// Candidate data prepared by the application boundary. The optional identity
/// is computed from a parsed contract; a host-attested claim must never be put
/// into this field when parsing fails.
#[derive(Debug, Clone)]
pub struct PreparedQualityCandidate {
    pub candidate_fingerprint: Option<String>,
    pub evidence: CandidateEvidence,
    pub proposal_change_paths: Vec<String>,
    pub prevalidation_diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityAssessmentResult {
    pub candidate_assessments: Vec<CandidateAssessment>,
    pub pareto_fronts: Vec<ParetoFront>,
}

/// Validate policy once at the application boundary, before any candidate is
/// assessed. Core assessment accepts only this validated representation.
pub fn validate_quality_policy(
    policy: &QualityPolicy,
) -> Result<ValidatedQualityPolicy, Vec<Diagnostic>> {
    let policy = policy.normalized();
    let mut diagnostics = Vec::new();

    validate_id(&policy.id, "/policy/id", &mut diagnostics);
    validate_fingerprint(
        &policy.claimed_evaluator_profile_fingerprint,
        "/policy/claimed_evaluator_profile_fingerprint",
        TQX_QUALITY_POLICY_INVALID,
        &mut diagnostics,
    );
    validate_fingerprint(
        &policy.claimed_model_profile_fingerprint,
        "/policy/claimed_model_profile_fingerprint",
        TQX_QUALITY_POLICY_INVALID,
        &mut diagnostics,
    );
    if policy.replicates_per_fixture == 0 {
        push(
            &mut diagnostics,
            TQX_QUALITY_POLICY_INVALID,
            "/policy/replicates_per_fixture",
        );
    } else if policy.replicates_per_fixture > QUALITY_MAX_REPLICATES {
        push(
            &mut diagnostics,
            TQX_QUALITY_LIMIT_EXCEEDED,
            "/policy/replicates_per_fixture",
        );
    }
    if policy.maximum_infrastructure_failure_ppm > PPM as u64 {
        push(
            &mut diagnostics,
            TQX_QUALITY_POLICY_INVALID,
            "/policy/maximum_infrastructure_failure_ppm",
        );
    }
    if policy.minimum_semantic_cases > QUALITY_MAX_PUBLIC_INTEGER {
        push(
            &mut diagnostics,
            TQX_QUALITY_LIMIT_EXCEEDED,
            "/policy/minimum_semantic_cases",
        );
    }
    if policy.binary_scorers.is_empty() || policy.binary_scorers.len() > QUALITY_MAX_SCORERS {
        push(
            &mut diagnostics,
            if policy.binary_scorers.len() > QUALITY_MAX_SCORERS {
                TQX_QUALITY_LIMIT_EXCEEDED
            } else {
                TQX_QUALITY_POLICY_INVALID
            },
            "/policy/binary_scorers",
        );
    }
    if policy.objectives.is_empty() || policy.objectives.len() > QUALITY_MAX_OBJECTIVES {
        push(
            &mut diagnostics,
            if policy.objectives.len() > QUALITY_MAX_OBJECTIVES {
                TQX_QUALITY_LIMIT_EXCEEDED
            } else {
                TQX_QUALITY_POLICY_INVALID
            },
            "/policy/objectives",
        );
    }
    if policy.eligibility_rules.is_empty() {
        push(
            &mut diagnostics,
            TQX_QUALITY_POLICY_INVALID,
            "/policy/eligibility_rules",
        );
    }

    let mut scorer_ids = BTreeSet::new();
    let mut scorer_metrics = BTreeSet::new();
    let mut scorers = BTreeMap::new();
    for (index, scorer) in policy.binary_scorers.iter().enumerate() {
        validate_id(
            &scorer.id,
            &format!("/policy/binary_scorers/{index}/id"),
            &mut diagnostics,
        );
        validate_id(
            &scorer.metric_id,
            &format!("/policy/binary_scorers/{index}/metric_id"),
            &mut diagnostics,
        );
        validate_fingerprint(
            &scorer.claimed_scorer_fingerprint,
            &format!("/policy/binary_scorers/{index}/claimed_scorer_fingerprint"),
            TQX_QUALITY_POLICY_INVALID,
            &mut diagnostics,
        );
        if !scorer_ids.insert(scorer.id.clone()) || !scorer_metrics.insert(scorer.metric_id.clone())
        {
            push(
                &mut diagnostics,
                TQX_QUALITY_POLICY_INVALID,
                format!("/policy/binary_scorers/{index}"),
            );
        }
        scorers.insert(scorer.id.clone(), scorer.clone());
    }

    let mut objective_ids = BTreeSet::new();
    let mut objective_metrics = BTreeSet::new();
    let mut objectives = BTreeMap::new();
    for (index, objective) in policy.objectives.iter().enumerate() {
        let path = format!("/policy/objectives/{index}");
        validate_id(&objective.id, &format!("{path}/id"), &mut diagnostics);
        validate_id(
            &objective.metric_id,
            &format!("{path}/metric_id"),
            &mut diagnostics,
        );
        validate_fingerprint(
            &objective.claimed_measurement_profile_fingerprint,
            &format!("{path}/claimed_measurement_profile_fingerprint"),
            TQX_QUALITY_POLICY_INVALID,
            &mut diagnostics,
        );
        if !objective_ids.insert(objective.id.clone())
            || !objective_metrics.insert(objective.metric_id.clone())
        {
            push(&mut diagnostics, TQX_QUALITY_POLICY_INVALID, &path);
        }
        validate_objective_shape(objective, &path, &scorer_metrics, &mut diagnostics);
        objectives.insert(objective.metric_id.clone(), objective.clone());
    }

    let mut rule_ids = BTreeSet::new();
    let mut rules_by_metric: BTreeMap<&str, Vec<&EligibilityRule>> = BTreeMap::new();
    for (index, rule) in policy.eligibility_rules.iter().enumerate() {
        let path = format!("/policy/eligibility_rules/{index}");
        validate_id(&rule.id, &format!("{path}/id"), &mut diagnostics);
        validate_id(
            &rule.metric_id,
            &format!("{path}/metric_id"),
            &mut diagnostics,
        );
        if !rule_ids.insert(rule.id.clone()) {
            push(&mut diagnostics, TQX_QUALITY_POLICY_INVALID, &path);
        }
        match objectives.get(&rule.metric_id) {
            Some(objective) if objective.unit == rule.unit => {}
            _ => push(&mut diagnostics, TQX_QUALITY_POLICY_INVALID, &path),
        }
        if rule.unit == MetricUnit::RatioPpm && rule.threshold > PPM as u64 {
            push(&mut diagnostics, TQX_QUALITY_POLICY_INVALID, &path);
        }
        if rule.unit != MetricUnit::RatioPpm && rule.threshold > QUALITY_MAX_PUBLIC_INTEGER {
            push(&mut diagnostics, TQX_QUALITY_LIMIT_EXCEEDED, &path);
        }
        rules_by_metric
            .entry(&rule.metric_id)
            .or_default()
            .push(rule);
    }

    // A11/A13: every scorer has one independently-computed ratio objective and
    // exactly one non-zero GTE safety floor over that same metric.
    for scorer in scorers.values() {
        let objective_valid = objectives.get(&scorer.metric_id).is_some_and(|objective| {
            objective.aggregation == MetricAggregation::BinaryRatioPpm
                && objective.unit == MetricUnit::RatioPpm
                && objective.direction == ObjectiveDirection::Maximize
        });
        if !objective_valid {
            push(
                &mut diagnostics,
                TQX_QUALITY_POLICY_INVALID,
                format!("/policy/binary_scorers/{}/metric_id", scorer.id),
            );
        }
        let rules = rules_by_metric
            .get(scorer.metric_id.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        if rules.len() != 1
            || rules[0].comparator != EligibilityComparator::Gte
            || rules[0].unit != MetricUnit::RatioPpm
            || !(1..=PPM as u64).contains(&rules[0].threshold)
        {
            push(
                &mut diagnostics,
                TQX_QUALITY_POLICY_INVALID,
                format!("/policy/binary_scorers/{}/metric_id", scorer.id),
            );
        }
    }

    let diagnostics = bounded_quality_diagnostics(diagnostics);
    if diagnostics.is_empty() {
        Ok(ValidatedQualityPolicy {
            policy,
            scorers,
            objectives,
        })
    } else {
        Err(diagnostics)
    }
}

/// Assess already-prepared candidates. Duplicate fixture IDs are a request
/// error; candidate evidence errors produce an ineligible candidate assessment
/// so one bad proposal cannot hide the remaining proposals.
pub fn assess_quality_candidates(
    policy: &ValidatedQualityPolicy,
    fixture_ids: &[String],
    candidates: Vec<PreparedQualityCandidate>,
) -> Result<QualityAssessmentResult, Vec<Diagnostic>> {
    let fixture_ids = validate_fixture_universe(policy, fixture_ids)?;
    let mut candidates = candidates;
    candidates.sort_by(prepared_candidate_cmp);

    let mut assessments: Vec<_> = candidates
        .iter()
        .map(|candidate| assess_candidate(policy, &fixture_ids, candidate))
        .collect();

    // A computed candidate identity may occur only once in a request. Mark all
    // repetitions ineligible rather than picking one based on input order.
    let mut computed_counts = BTreeMap::new();
    let mut claimed_counts = BTreeMap::new();
    for candidate in &candidates {
        if let Some(fingerprint) = &candidate.candidate_fingerprint {
            *computed_counts.entry(fingerprint.clone()).or_insert(0_u32) += 1;
        }
        *claimed_counts
            .entry(
                candidate
                    .evidence
                    .claimed_candidate_contract_fingerprint
                    .clone(),
            )
            .or_insert(0_u32) += 1;
    }
    for (assessment, candidate) in assessments.iter_mut().zip(&candidates) {
        let duplicate_computed = assessment
            .candidate_fingerprint
            .as_ref()
            .is_some_and(|fingerprint| computed_counts[fingerprint] > 1);
        let duplicate_claimed =
            claimed_counts[&candidate.evidence.claimed_candidate_contract_fingerprint] > 1;
        if duplicate_computed || duplicate_claimed {
            assessment.diagnostics.push(quality_diagnostic(
                TQX_QUALITY_EVIDENCE_INVALID,
                if duplicate_claimed {
                    "/evidence/claimed_candidate_contract_fingerprint"
                } else {
                    "/candidate_fingerprint"
                },
            ));
            assessment.diagnostics =
                bounded_quality_diagnostics(std::mem::take(&mut assessment.diagnostics));
            assessment.eligibility.eligible = false;
        }
    }
    assessments.sort_by(assessment_cmp);
    let pareto_fronts = compute_pareto_fronts(policy, &assessments);
    Ok(QualityAssessmentResult {
        candidate_assessments: assessments,
        pareto_fronts,
    })
}

fn validate_fixture_universe(
    policy: &ValidatedQualityPolicy,
    fixture_ids: &[String],
) -> Result<Vec<String>, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    if fixture_ids.is_empty() || fixture_ids.len() > QUALITY_MAX_FIXTURES {
        push(&mut diagnostics, TQX_QUALITY_LIMIT_EXCEEDED, "/fixtures");
    }
    let mut found = BTreeSet::new();
    for (index, id) in fixture_ids.iter().enumerate() {
        validate_evidence_id(id, &format!("/fixtures/{index}/id"), &mut diagnostics);
        if !found.insert(id.clone()) {
            push(
                &mut diagnostics,
                TQX_QUALITY_EVIDENCE_INVALID,
                format!("/fixtures/{index}/id"),
            );
        }
    }
    let trial_count = fixture_ids
        .len()
        .checked_mul(usize::from(policy.policy.replicates_per_fixture));
    if trial_count.is_none_or(|count| count > QUALITY_MAX_TRIALS_PER_CANDIDATE) {
        push(&mut diagnostics, TQX_QUALITY_LIMIT_EXCEEDED, "/fixtures");
    }
    if diagnostics.is_empty() {
        Ok(found.into_iter().collect())
    } else {
        Err(bounded_quality_diagnostics(diagnostics))
    }
}

fn assess_candidate(
    policy: &ValidatedQualityPolicy,
    fixture_ids: &[String],
    candidate: &PreparedQualityCandidate,
) -> CandidateAssessment {
    let mut diagnostics: Vec<_> = candidate
        .prevalidation_diagnostics
        .iter()
        .map(sanitize_diagnostic)
        .collect();
    if candidate.candidate_fingerprint.is_none() {
        push(
            &mut diagnostics,
            TQX_QUALITY_CANDIDATE_INVALID,
            "/candidate_source",
        );
    } else if let Some(fingerprint) = &candidate.candidate_fingerprint {
        validate_fingerprint(
            fingerprint,
            "/candidate_fingerprint",
            TQX_QUALITY_EVIDENCE_INVALID,
            &mut diagnostics,
        );
        if fingerprint != &candidate.evidence.claimed_candidate_contract_fingerprint {
            push(
                &mut diagnostics,
                TQX_QUALITY_BINDING_MISMATCH,
                "/evidence/claimed_candidate_contract_fingerprint",
            );
        }
    }
    let evidence = candidate.evidence.normalized();
    let claims_valid = validate_candidate_claims(policy, &evidence, &mut diagnostics)
        && candidate
            .candidate_fingerprint
            .as_ref()
            .is_none_or(|fingerprint| {
                fingerprint == &evidence.claimed_candidate_contract_fingerprint
            });

    let expected_keys: BTreeSet<_> = fixture_ids
        .iter()
        .flat_map(|fixture_id| {
            (0..policy.policy.replicates_per_fixture)
                .map(move |replicate| (fixture_id.clone(), replicate))
        })
        .collect();
    let structural_valid = validate_trials(
        policy,
        &evidence.trials,
        &evidence.claimed_measurement_profile_fingerprints,
        &expected_keys,
        &mut diagnostics,
    );

    // Invalid duplicate/extra retry rows never inflate a coverage ratio. Count
    // at most one submitted row for each expected logical trial key.
    let mut counted_keys = BTreeSet::new();
    let logical_trials: Vec<_> = evidence
        .trials
        .iter()
        .filter(|trial| {
            let key = (trial.fixture_id.clone(), trial.replicate_index);
            expected_keys.contains(&key) && counted_keys.insert(key)
        })
        .collect();
    let total = u64::try_from(expected_keys.len())
        .expect("validated quality trial cardinality always fits in u64");
    let semantic = logical_trials
        .iter()
        .filter(|trial| !matches!(trial.outcome, TrialOutcome::InfrastructureFailure { .. }))
        .count() as u64;
    let infrastructure = logical_trials
        .iter()
        .filter(|trial| matches!(trial.outcome, TrialOutcome::InfrastructureFailure { .. }))
        .count() as u64;
    let semantic_coverage_ppm = ratio_ppm(semantic, total, &mut diagnostics, "/trials");
    let infrastructure_failure_ppm = ratio_ppm(infrastructure, total, &mut diagnostics, "/trials");

    let aggregates = if structural_valid {
        aggregate_metrics(policy, &evidence.trials, semantic, &mut diagnostics)
    } else {
        Vec::new()
    };
    let aggregate_by_metric: BTreeMap<_, _> = aggregates
        .iter()
        .map(|aggregate| (aggregate.metric_id.as_str(), aggregate.value))
        .collect();

    let mut gates = Vec::new();
    for rule in &policy.policy.eligibility_rules {
        let actual = aggregate_by_metric.get(rule.metric_id.as_str()).copied();
        gates.push(EligibilityGate {
            rule_id: rule.id.clone(),
            passed: actual.is_some_and(|value| compare(value, rule)),
            actual,
            comparator: rule.comparator,
            threshold: rule.threshold,
            unit: rule.unit,
        });
    }
    gates.sort_by(|left, right| left.rule_id.cmp(&right.rule_id));

    if semantic == 0 || semantic < policy.policy.minimum_semantic_cases {
        push(
            &mut diagnostics,
            TQX_QUALITY_INSUFFICIENT_COVERAGE,
            "/evidence/trials",
        );
    }
    if infrastructure_failure_ppm > policy.policy.maximum_infrastructure_failure_ppm {
        push(
            &mut diagnostics,
            TQX_QUALITY_INFRASTRUCTURE_BUDGET,
            "/evidence/trials",
        );
    }
    for gate in &gates {
        if !gate.passed {
            push(
                &mut diagnostics,
                TQX_QUALITY_GATE_FAILED,
                format!("/eligibility_rules/{}", gate.rule_id),
            );
        }
    }
    let mut proposal_change_paths = candidate.proposal_change_paths.clone();
    let invalid_change_path = proposal_change_paths
        .iter()
        .any(|path| !PROPOSAL_CHANGE_PATHS.contains(&path.as_str()));
    proposal_change_paths.retain(|path| PROPOSAL_CHANGE_PATHS.contains(&path.as_str()));
    proposal_change_paths.sort();
    proposal_change_paths.dedup();
    if invalid_change_path {
        push(
            &mut diagnostics,
            TQX_QUALITY_EVIDENCE_INVALID,
            "/proposal_change_paths",
        );
    }
    let diagnostics = bounded_quality_diagnostics(diagnostics);
    let eligible = structural_valid
        && candidate.candidate_fingerprint.is_some()
        && diagnostics.is_empty()
        && gates.iter().all(|gate| gate.passed);

    let claimed_identities = claims_valid.then(|| ClaimedQualityIdentities {
        claimed_candidate_contract_fingerprint: evidence
            .claimed_candidate_contract_fingerprint
            .clone(),
        claimed_evaluator_profile_fingerprint: evidence
            .claimed_evaluator_profile_fingerprint
            .clone(),
        claimed_model_profile_fingerprint: evidence.claimed_model_profile_fingerprint.clone(),
        claimed_scorer_fingerprints: evidence.claimed_scorer_fingerprints.clone(),
        claimed_measurement_profile_fingerprints: evidence
            .claimed_measurement_profile_fingerprints
            .clone(),
    });
    let trial_summaries = if structural_valid {
        evidence.trials.into_iter().map(trial_summary).collect()
    } else {
        Vec::new()
    };
    CandidateAssessment {
        candidate_fingerprint: candidate.candidate_fingerprint.clone(),
        claimed_identities,
        eligibility: EligibilityAssessment {
            eligible,
            total_trial_count: total,
            semantic_trial_count: semantic,
            infrastructure_trial_count: infrastructure,
            semantic_coverage_ppm,
            infrastructure_failure_ppm,
            gates,
        },
        aggregates,
        trial_summaries,
        proposal_change_paths,
        diagnostics,
    }
}

fn validate_candidate_claims(
    policy: &ValidatedQualityPolicy,
    evidence: &CandidateEvidence,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let before = diagnostics.len();
    for (value, path) in [
        (
            &evidence.claimed_package_fingerprint,
            "/evidence/claimed_package_fingerprint",
        ),
        (
            &evidence.claimed_base_contract_fingerprint,
            "/evidence/claimed_base_contract_fingerprint",
        ),
        (
            &evidence.claimed_fixture_set_fingerprint,
            "/evidence/claimed_fixture_set_fingerprint",
        ),
        (
            &evidence.claimed_candidate_contract_fingerprint,
            "/evidence/claimed_candidate_contract_fingerprint",
        ),
        (
            &evidence.claimed_quality_policy_fingerprint,
            "/evidence/claimed_quality_policy_fingerprint",
        ),
        (
            &evidence.claimed_evaluator_profile_fingerprint,
            "/evidence/claimed_evaluator_profile_fingerprint",
        ),
        (
            &evidence.claimed_model_profile_fingerprint,
            "/evidence/claimed_model_profile_fingerprint",
        ),
    ] {
        validate_fingerprint(value, path, TQX_QUALITY_EVIDENCE_INVALID, diagnostics);
    }
    if evidence.claimed_evaluator_profile_fingerprint
        != policy.policy.claimed_evaluator_profile_fingerprint
    {
        push(
            diagnostics,
            TQX_QUALITY_EVIDENCE_INVALID,
            "/evidence/claimed_evaluator_profile_fingerprint",
        );
    }
    if evidence.claimed_model_profile_fingerprint != policy.policy.claimed_model_profile_fingerprint
    {
        push(
            diagnostics,
            TQX_QUALITY_EVIDENCE_INVALID,
            "/evidence/claimed_model_profile_fingerprint",
        );
    }
    if evidence.claimed_scorer_fingerprints.len() != policy.scorers.len() {
        push(
            diagnostics,
            TQX_QUALITY_EVIDENCE_INVALID,
            "/evidence/claimed_scorer_fingerprints",
        );
    }
    if evidence.claimed_scorer_fingerprints.len() > QUALITY_MAX_SCORERS {
        push(
            diagnostics,
            TQX_QUALITY_LIMIT_EXCEEDED,
            "/evidence/claimed_scorer_fingerprints",
        );
    }
    for scorer in policy.scorers.values() {
        match evidence.claimed_scorer_fingerprints.get(&scorer.id) {
            Some(claim) => {
                validate_fingerprint(
                    claim,
                    &format!("/evidence/claimed_scorer_fingerprints/{}", scorer.id),
                    TQX_QUALITY_EVIDENCE_INVALID,
                    diagnostics,
                );
                if claim != &scorer.claimed_scorer_fingerprint {
                    push(
                        diagnostics,
                        TQX_QUALITY_EVIDENCE_INVALID,
                        format!("/evidence/claimed_scorer_fingerprints/{}", scorer.id),
                    );
                }
            }
            None => push(
                diagnostics,
                TQX_QUALITY_EVIDENCE_INVALID,
                format!("/evidence/claimed_scorer_fingerprints/{}", scorer.id),
            ),
        }
    }
    for id in evidence.claimed_scorer_fingerprints.keys() {
        validate_evidence_id(id, "/evidence/claimed_scorer_fingerprints", diagnostics);
        if !policy.scorers.contains_key(id) {
            push(
                diagnostics,
                TQX_QUALITY_EVIDENCE_INVALID,
                "/evidence/claimed_scorer_fingerprints",
            );
        }
    }
    if evidence.claimed_measurement_profile_fingerprints.len() != policy.objectives.len() {
        push(
            diagnostics,
            TQX_QUALITY_EVIDENCE_INVALID,
            "/evidence/claimed_measurement_profile_fingerprints",
        );
    }
    if evidence.claimed_measurement_profile_fingerprints.len() > QUALITY_MAX_OBJECTIVES {
        push(
            diagnostics,
            TQX_QUALITY_LIMIT_EXCEEDED,
            "/evidence/claimed_measurement_profile_fingerprints",
        );
    }
    for objective in policy.objectives.values() {
        let path = format!(
            "/evidence/claimed_measurement_profile_fingerprints/{}",
            objective.metric_id
        );
        match evidence
            .claimed_measurement_profile_fingerprints
            .get(&objective.metric_id)
        {
            Some(claim) => {
                validate_fingerprint(claim, &path, TQX_QUALITY_EVIDENCE_INVALID, diagnostics);
                if claim != &objective.claimed_measurement_profile_fingerprint {
                    push(diagnostics, TQX_QUALITY_EVIDENCE_INVALID, &path);
                }
            }
            None => push(diagnostics, TQX_QUALITY_EVIDENCE_INVALID, &path),
        }
    }
    for metric_id in evidence.claimed_measurement_profile_fingerprints.keys() {
        validate_evidence_id(
            metric_id,
            "/evidence/claimed_measurement_profile_fingerprints",
            diagnostics,
        );
        if !policy.objectives.contains_key(metric_id) {
            push(
                diagnostics,
                TQX_QUALITY_EVIDENCE_INVALID,
                "/evidence/claimed_measurement_profile_fingerprints",
            );
        }
    }
    diagnostics.len() == before
}

fn validate_trials(
    policy: &ValidatedQualityPolicy,
    trials: &[TrialEvidence],
    measurement_claims: &BTreeMap<String, String>,
    expected_keys: &BTreeSet<(String, u16)>,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let before = diagnostics.len();
    if trials.len() != expected_keys.len() {
        push(
            diagnostics,
            TQX_QUALITY_EVIDENCE_INVALID,
            "/evidence/trials",
        );
    }
    if trials.len() > QUALITY_MAX_TRIALS_PER_CANDIDATE {
        push(diagnostics, TQX_QUALITY_LIMIT_EXCEEDED, "/evidence/trials");
    }
    let scorer_ids: BTreeSet<_> = policy.scorers.keys().cloned().collect();
    let resource_metrics: BTreeSet<_> = policy
        .objectives
        .values()
        .filter(|objective| objective.aggregation != MetricAggregation::BinaryRatioPpm)
        .map(|objective| objective.metric_id.clone())
        .collect();
    let mut keys = BTreeSet::new();
    for (index, trial) in trials.iter().enumerate() {
        let path = format!("/evidence/trials/{index}");
        validate_evidence_id(
            &trial.fixture_id,
            &format!("{path}/fixture_id"),
            diagnostics,
        );
        let key = (trial.fixture_id.clone(), trial.replicate_index);
        if !keys.insert(key.clone()) || !expected_keys.contains(&key) {
            push(diagnostics, TQX_QUALITY_EVIDENCE_INVALID, &path);
        }
        if trial.provider_attempt_count == 0 {
            push(
                diagnostics,
                TQX_QUALITY_EVIDENCE_INVALID,
                format!("{path}/provider_attempt_count"),
            );
        }
        if trial.observations.len() > QUALITY_MAX_OBSERVATIONS_PER_TRIAL {
            push(
                diagnostics,
                TQX_QUALITY_LIMIT_EXCEEDED,
                format!("{path}/observations"),
            );
        }
        let passed: BTreeSet<_> = trial.passed_scorers.iter().cloned().collect();
        let failed: BTreeSet<_> = trial.failed_scorers.iter().cloned().collect();
        if trial.passed_scorers.len() > QUALITY_MAX_SCORERS
            || trial.failed_scorers.len() > QUALITY_MAX_SCORERS
        {
            push(
                diagnostics,
                TQX_QUALITY_LIMIT_EXCEEDED,
                format!("{path}/passed_scorers"),
            );
        }
        for scorer_id in trial.passed_scorers.iter().chain(&trial.failed_scorers) {
            validate_evidence_id(scorer_id, &format!("{path}/scorers"), diagnostics);
        }
        let scorer_duplicates = passed.len() != trial.passed_scorers.len()
            || failed.len() != trial.failed_scorers.len()
            || !passed.is_disjoint(&failed);
        match trial.outcome {
            TrialOutcome::Scored | TrialOutcome::CandidateQualityFailure { .. } => {
                let union: BTreeSet<_> = passed.union(&failed).cloned().collect();
                if scorer_duplicates || union != scorer_ids {
                    push(
                        diagnostics,
                        TQX_QUALITY_EVIDENCE_INVALID,
                        format!("{path}/passed_scorers"),
                    );
                }
                if matches!(trial.outcome, TrialOutcome::CandidateQualityFailure { .. })
                    && failed.is_empty()
                {
                    push(
                        diagnostics,
                        TQX_QUALITY_EVIDENCE_INVALID,
                        format!("{path}/failed_scorers"),
                    );
                }
            }
            TrialOutcome::InfrastructureFailure { .. } => {
                if !passed.is_empty() || !failed.is_empty() {
                    push(
                        diagnostics,
                        TQX_QUALITY_EVIDENCE_INVALID,
                        format!("{path}/passed_scorers"),
                    );
                }
            }
        }
        validate_observations(
            policy,
            &trial.observations,
            measurement_claims,
            &resource_metrics,
            &path,
            diagnostics,
        );
    }
    if keys != *expected_keys {
        push(
            diagnostics,
            TQX_QUALITY_EVIDENCE_INVALID,
            "/evidence/trials",
        );
    }
    diagnostics.len() == before
}

fn validate_observations(
    policy: &ValidatedQualityPolicy,
    observations: &[MetricObservation],
    measurement_claims: &BTreeMap<String, String>,
    resource_metrics: &BTreeSet<String>,
    trial_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut found = BTreeSet::new();
    for (index, observation) in observations.iter().enumerate() {
        let path = format!("{trial_path}/observations/{index}");
        validate_evidence_id(
            &observation.metric_id,
            &format!("{path}/metric_id"),
            diagnostics,
        );
        if !found.insert(observation.metric_id.clone()) {
            push(diagnostics, TQX_QUALITY_EVIDENCE_INVALID, &path);
            continue;
        }
        let Some(objective) = policy.objectives.get(&observation.metric_id) else {
            push(diagnostics, TQX_QUALITY_EVIDENCE_INVALID, &path);
            continue;
        };
        if objective.aggregation == MetricAggregation::BinaryRatioPpm {
            push(diagnostics, TQX_QUALITY_EVIDENCE_INVALID, &path);
            continue;
        }
        if observation.unit != objective.unit {
            push(
                diagnostics,
                TQX_QUALITY_METRIC_UNIT_MISMATCH,
                format!("{path}/unit"),
            );
        }
        if observation.claimed_measurement_profile_fingerprint
            != objective.claimed_measurement_profile_fingerprint
            || measurement_claims
                .get(&observation.metric_id)
                .map(String::as_str)
                != Some(observation.claimed_measurement_profile_fingerprint.as_str())
        {
            push(
                diagnostics,
                TQX_QUALITY_EVIDENCE_INVALID,
                format!("{path}/claimed_measurement_profile_fingerprint"),
            );
        }
        validate_fingerprint(
            &observation.claimed_measurement_profile_fingerprint,
            &format!("{path}/claimed_measurement_profile_fingerprint"),
            TQX_QUALITY_EVIDENCE_INVALID,
            diagnostics,
        );
        if observation.currency_code != objective.currency_code
            || observation.token_kind != objective.token_kind
        {
            push(diagnostics, TQX_QUALITY_EVIDENCE_INVALID, &path);
        }
        if observation.value > QUALITY_MAX_PUBLIC_INTEGER {
            push(
                diagnostics,
                TQX_QUALITY_LIMIT_EXCEEDED,
                format!("{path}/value"),
            );
        }
    }
    for metric in resource_metrics {
        if !found.contains(metric) {
            push(
                diagnostics,
                TQX_QUALITY_METRIC_MISSING,
                format!("{trial_path}/observations/{metric}"),
            );
        }
    }
}

fn aggregate_metrics(
    policy: &ValidatedQualityPolicy,
    trials: &[TrialEvidence],
    semantic_count: u64,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<MetricAggregate> {
    let mut aggregates = Vec::new();
    for objective in policy.objectives.values() {
        let value = if objective.aggregation == MetricAggregation::BinaryRatioPpm {
            if semantic_count == 0 {
                push(
                    diagnostics,
                    TQX_QUALITY_INSUFFICIENT_COVERAGE,
                    format!("/aggregates/{}", objective.metric_id),
                );
                continue;
            }
            let Some(scorer) = policy
                .scorers
                .values()
                .find(|scorer| scorer.metric_id == objective.metric_id)
            else {
                push(
                    diagnostics,
                    TQX_QUALITY_POLICY_INVALID,
                    format!("/policy/objectives/{}/metric_id", objective.id),
                );
                continue;
            };
            let passed = trials
                .iter()
                .filter(|trial| {
                    !matches!(trial.outcome, TrialOutcome::InfrastructureFailure { .. })
                        && trial.passed_scorers.contains(&scorer.id)
                })
                .count() as u64;
            ratio_ppm(
                passed,
                semantic_count,
                diagnostics,
                &format!("/aggregates/{}", objective.metric_id),
            )
        } else {
            let values: Vec<u64> = trials
                .iter()
                .filter_map(|trial| {
                    trial
                        .observations
                        .iter()
                        .find(|observation| observation.metric_id == objective.metric_id)
                        .map(|observation| observation.value)
                })
                .collect();
            match aggregate_resource(objective.aggregation, &values) {
                Ok(value) => value,
                Err(code) => {
                    push(
                        diagnostics,
                        code,
                        format!("/aggregates/{}", objective.metric_id),
                    );
                    continue;
                }
            }
        };
        aggregates.push(MetricAggregate {
            metric_id: objective.metric_id.clone(),
            unit: objective.unit,
            aggregation: objective.aggregation,
            direction: objective.direction,
            value,
        });
    }
    aggregates.sort_by(|left, right| left.metric_id.cmp(&right.metric_id));
    aggregates
}

fn aggregate_resource(aggregation: MetricAggregation, values: &[u64]) -> Result<u64, &'static str> {
    if values.is_empty() {
        return Err(TQX_QUALITY_EVIDENCE_INVALID);
    }
    let sum = values
        .iter()
        .try_fold(0_u128, |total, value| total.checked_add(u128::from(*value)))
        .ok_or(TQX_QUALITY_LIMIT_EXCEEDED)?;
    let result = match aggregation {
        MetricAggregation::Mean => sum
            .checked_div(values.len() as u128)
            .ok_or(TQX_QUALITY_LIMIT_EXCEEDED)?,
        MetricAggregation::Sum => sum,
        MetricAggregation::P95NearestRank => {
            let mut sorted = values.to_vec();
            sorted.sort_unstable();
            let rank = (95_u128
                .checked_mul(sorted.len() as u128)
                .and_then(|value| value.checked_add(99))
                .ok_or(TQX_QUALITY_LIMIT_EXCEEDED)?)
                / 100;
            return sorted
                .get(
                    usize::try_from(rank.checked_sub(1).ok_or(TQX_QUALITY_LIMIT_EXCEEDED)?)
                        .map_err(|_| TQX_QUALITY_LIMIT_EXCEEDED)?,
                )
                .copied()
                .ok_or(TQX_QUALITY_EVIDENCE_INVALID);
        }
        MetricAggregation::BinaryRatioPpm => return Err(TQX_QUALITY_EVIDENCE_INVALID),
    };
    if result > u128::from(QUALITY_MAX_PUBLIC_INTEGER) {
        return Err(TQX_QUALITY_LIMIT_EXCEEDED);
    }
    u64::try_from(result).map_err(|_| TQX_QUALITY_LIMIT_EXCEEDED)
}

fn compute_pareto_fronts(
    policy: &ValidatedQualityPolicy,
    assessments: &[CandidateAssessment],
) -> Vec<ParetoFront> {
    let mut remaining: Vec<_> = assessments
        .iter()
        .filter(|assessment| assessment.eligibility.eligible)
        .collect();
    remaining.sort_by(assessment_ref_cmp);
    let mut fronts = Vec::new();
    let mut rank = 1_u32;
    while !remaining.is_empty() {
        let front: Vec<_> = remaining
            .iter()
            .copied()
            .filter(|candidate| {
                !remaining.iter().copied().any(|other| {
                    !std::ptr::eq(*candidate, other) && dominates(policy, other, candidate)
                })
            })
            .collect();
        let fingerprints: Vec<_> = front
            .iter()
            .filter_map(|assessment| assessment.candidate_fingerprint.clone())
            .collect();
        fronts.push(ParetoFront {
            rank,
            candidate_fingerprints: fingerprints,
        });
        remaining.retain(|candidate| !front.iter().any(|item| std::ptr::eq(*item, *candidate)));
        rank += 1;
    }
    fronts
}

fn dominates(
    policy: &ValidatedQualityPolicy,
    left: &CandidateAssessment,
    right: &CandidateAssessment,
) -> bool {
    let left_values: BTreeMap<_, _> = left
        .aggregates
        .iter()
        .map(|aggregate| (&aggregate.metric_id, aggregate.value))
        .collect();
    let right_values: BTreeMap<_, _> = right
        .aggregates
        .iter()
        .map(|aggregate| (&aggregate.metric_id, aggregate.value))
        .collect();
    let mut strictly_better = false;
    for objective in policy.objectives.values() {
        let (Some(left), Some(right)) = (
            left_values.get(&objective.metric_id),
            right_values.get(&objective.metric_id),
        ) else {
            return false;
        };
        match objective.direction {
            ObjectiveDirection::Maximize if left < right => return false,
            ObjectiveDirection::Minimize if left > right => return false,
            ObjectiveDirection::Maximize if left > right => strictly_better = true,
            ObjectiveDirection::Minimize if left < right => strictly_better = true,
            _ => {}
        }
    }
    strictly_better
}

/// Sort, deduplicate by stable `(code,path)`, and apply A12/A13's exact 256
/// diagnostic boundary. Messages and source-bearing metadata are redacted.
#[must_use]
pub fn bounded_quality_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<_> = diagnostics.iter().map(sanitize_diagnostic).collect();
    diagnostics.sort_by(|left, right| diagnostic_key(left).cmp(&diagnostic_key(right)));
    diagnostics.dedup_by(|left, right| diagnostic_key(left) == diagnostic_key(right));
    if diagnostics.len() > QUALITY_MAX_DIAGNOSTICS {
        diagnostics.truncate(QUALITY_MAX_DIAGNOSTICS - 1);
        diagnostics.push(quality_diagnostic(TQX_QUALITY_DIAGNOSTICS_TRUNCATED, "/"));
    }
    diagnostics
}

fn validate_objective_shape(
    objective: &QualityObjective,
    path: &str,
    scorer_metrics: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let valid = match objective.unit {
        MetricUnit::RatioPpm => {
            objective.aggregation == MetricAggregation::BinaryRatioPpm
                && objective.direction == ObjectiveDirection::Maximize
                && scorer_metrics.contains(&objective.metric_id)
                && objective.currency_code.is_none()
                && objective.token_kind.is_none()
        }
        MetricUnit::Milliseconds => {
            objective.aggregation != MetricAggregation::BinaryRatioPpm
                && objective.currency_code.is_none()
                && objective.token_kind.is_none()
        }
        MetricUnit::TokenCount => {
            objective.aggregation != MetricAggregation::BinaryRatioPpm
                && objective.currency_code.is_none()
                && objective.token_kind.is_some()
        }
        MetricUnit::CurrencyMicrounits => {
            objective.aggregation != MetricAggregation::BinaryRatioPpm
                && objective.token_kind.is_none()
                && objective
                    .currency_code
                    .as_deref()
                    .is_some_and(valid_currency)
        }
    };
    if !valid {
        push(diagnostics, TQX_QUALITY_POLICY_INVALID, path);
    }
}

fn valid_currency(value: &str) -> bool {
    // Protocol-v1's deterministic ISO-4217 alphabetic-code set, frozen for
    // reproducible validation rather than delegated to locale/finance state.
    // It includes active currencies, fund codes, precious metals, testing,
    // and the ISO no-currency code as published for the 2026 protocol line.

    ISO_4217_CODES.binary_search(&value).is_ok()
}

fn compare(value: u64, rule: &EligibilityRule) -> bool {
    match rule.comparator {
        EligibilityComparator::Gte => value >= rule.threshold,
        EligibilityComparator::Lte => value <= rule.threshold,
    }
}

fn ratio_ppm(
    numerator: u64,
    denominator: u64,
    diagnostics: &mut Vec<Diagnostic>,
    path: &str,
) -> u64 {
    let value = u128::from(numerator)
        .checked_mul(PPM)
        .and_then(|value| value.checked_div(u128::from(denominator)))
        .and_then(|value| u64::try_from(value).ok());
    match value {
        Some(value) => value,
        None => {
            push(diagnostics, TQX_QUALITY_EVIDENCE_INVALID, path);
            0
        }
    }
}

fn trial_summary(trial: TrialEvidence) -> QualityTrialSummary {
    QualityTrialSummary {
        fixture_id: trial.fixture_id,
        replicate_index: trial.replicate_index,
        provider_attempt_count: trial.provider_attempt_count,
        outcome: trial.outcome,
        passed_scorers: trial.passed_scorers,
        failed_scorers: trial.failed_scorers,
        observations: trial.observations,
    }
}

fn validate_id(value: &str, path: &str, diagnostics: &mut Vec<Diagnostic>) {
    validate_id_with_code(value, path, TQX_QUALITY_POLICY_INVALID, diagnostics);
}

fn validate_evidence_id(value: &str, path: &str, diagnostics: &mut Vec<Diagnostic>) {
    validate_id_with_code(value, path, TQX_QUALITY_EVIDENCE_INVALID, diagnostics);
}

fn validate_id_with_code(value: &str, path: &str, code: &str, diagnostics: &mut Vec<Diagnostic>) {
    if value.len() > QUALITY_MAX_ID_BYTES {
        push(diagnostics, TQX_QUALITY_LIMIT_EXCEEDED, path);
    } else if !valid_id_shape(value) {
        push(diagnostics, code, path);
    }
}

fn valid_id_shape(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= QUALITY_MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn validate_fingerprint(value: &str, path: &str, code: &str, diagnostics: &mut Vec<Diagnostic>) {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        push(diagnostics, code, path);
    }
}

fn prepared_candidate_cmp(
    left: &PreparedQualityCandidate,
    right: &PreparedQualityCandidate,
) -> std::cmp::Ordering {
    candidate_sort_key(left.candidate_fingerprint.as_ref(), &left.evidence)
        .cmp(candidate_sort_key(
            right.candidate_fingerprint.as_ref(),
            &right.evidence,
        ))
        .then_with(|| {
            templiqx_contracts::canonical_json(&left.evidence)
                .expect("quality evidence serialization is infallible")
                .cmp(
                    &templiqx_contracts::canonical_json(&right.evidence)
                        .expect("quality evidence serialization is infallible"),
                )
        })
}

fn candidate_sort_key<'a>(
    computed: Option<&'a String>,
    evidence: &'a CandidateEvidence,
) -> &'a str {
    computed
        .map(String::as_str)
        .unwrap_or(&evidence.claimed_candidate_contract_fingerprint)
}

fn assessment_cmp(left: &CandidateAssessment, right: &CandidateAssessment) -> std::cmp::Ordering {
    assessment_sort_key(left)
        .cmp(&assessment_sort_key(right))
        .then_with(|| {
            templiqx_contracts::canonical_json(left)
                .expect("quality assessment serialization is infallible")
                .cmp(
                    &templiqx_contracts::canonical_json(right)
                        .expect("quality assessment serialization is infallible"),
                )
        })
}

fn assessment_ref_cmp(
    left: &&CandidateAssessment,
    right: &&CandidateAssessment,
) -> std::cmp::Ordering {
    assessment_cmp(left, right)
}

fn assessment_sort_key(assessment: &CandidateAssessment) -> Option<&str> {
    assessment.candidate_fingerprint.as_deref().or_else(|| {
        assessment
            .claimed_identities
            .as_ref()
            .map(|identities| identities.claimed_candidate_contract_fingerprint.as_str())
    })
}

fn diagnostic_key(diagnostic: &Diagnostic) -> (&str, &str) {
    (
        diagnostic.code.as_str(),
        diagnostic.json_pointer.as_deref().unwrap_or("/"),
    )
}

fn sanitize_diagnostic(diagnostic: &Diagnostic) -> Diagnostic {
    quality_diagnostic(
        &diagnostic.code,
        diagnostic.json_pointer.as_deref().unwrap_or("/"),
    )
}

fn quality_diagnostic(code: &str, pointer: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: code.to_owned(),
        severity: Severity::Error,
        message: "quality proposal is invalid".into(),
        file: None,
        json_pointer: Some(pointer.into()),
        span: None,
        help: None,
    }
}

fn push(diagnostics: &mut Vec<Diagnostic>, code: &str, pointer: impl Into<String>) {
    diagnostics.push(quality_diagnostic(code, pointer));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_iso_4217_snapshot_has_protocol_goldens() {
        assert_eq!(ISO_4217_CODES.len(), ISO_4217_CODE_COUNT);
        assert_eq!(ISO_4217_CODES.first(), Some(&"AED"));
        assert_eq!(ISO_4217_CODES.last(), Some(&"ZWG"));
        for member in ["USD", "EUR", "XTS"] {
            assert!(valid_currency(member));
        }
        for nonmember in ["ZZZ", "CUC"] {
            assert!(!valid_currency(nonmember));
        }
        assert_eq!(ISO_4217_SNAPSHOT_DATE, "2026-07-22");
        assert_eq!(
            ISO_4217_CODES_SHA256,
            "4c64388da43ddb82dbf818d7f68a5f8511da6a46bf2ef318d28d9e36eea0a8a7"
        );
        assert!(ISO_4217_SOURCE.starts_with("https://www.six-group.com/"));
    }
}
