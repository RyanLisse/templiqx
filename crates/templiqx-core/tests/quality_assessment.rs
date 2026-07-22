use templiqx_contracts::{
    BinaryScorer, CandidateEvidence, CandidateQualityFailureReason, Diagnostic,
    EligibilityComparator, EligibilityRule, InfrastructureFailureReason, MetricAggregation,
    MetricObservation, MetricUnit, ObjectiveDirection, QualityObjective, QualityPolicy, Severity,
    TQX_QUALITY_CANDIDATE_INVALID, TQX_QUALITY_DIAGNOSTICS_TRUNCATED, TQX_QUALITY_EVIDENCE_INVALID,
    TQX_QUALITY_LIMIT_EXCEEDED, TQX_QUALITY_POLICY_INVALID, TrialEvidence, TrialOutcome,
};
use templiqx_core::{
    PreparedQualityCandidate, assess_quality_candidates, bounded_quality_diagnostics,
    validate_quality_policy,
};

fn hash(ch: char) -> String {
    std::iter::repeat_n(ch, 64).collect()
}

fn scorer(id: &str, metric: &str, fingerprint: char) -> BinaryScorer {
    BinaryScorer {
        id: id.into(),
        metric_id: metric.into(),
        claimed_scorer_fingerprint: hash(fingerprint),
    }
}

fn objective(
    id: &str,
    metric: &str,
    unit: MetricUnit,
    aggregation: MetricAggregation,
    direction: ObjectiveDirection,
    fingerprint: char,
) -> QualityObjective {
    QualityObjective {
        id: id.into(),
        metric_id: metric.into(),
        unit,
        aggregation,
        direction,
        claimed_measurement_profile_fingerprint: hash(fingerprint),
        currency_code: None,
        token_kind: None,
    }
}

fn policy_two_scorers(floor: u64) -> QualityPolicy {
    QualityPolicy {
        id: "policy".into(),
        replicates_per_fixture: 1,
        minimum_semantic_cases: 1,
        maximum_infrastructure_failure_ppm: 1_000_000,
        claimed_evaluator_profile_fingerprint: hash('a'),
        claimed_model_profile_fingerprint: hash('b'),
        binary_scorers: vec![
            scorer("correctness", "correctness_ratio", 'c'),
            scorer("grounding", "grounding_ratio", 'd'),
        ],
        objectives: vec![
            objective(
                "correctness",
                "correctness_ratio",
                MetricUnit::RatioPpm,
                MetricAggregation::BinaryRatioPpm,
                ObjectiveDirection::Maximize,
                'e',
            ),
            objective(
                "grounding",
                "grounding_ratio",
                MetricUnit::RatioPpm,
                MetricAggregation::BinaryRatioPpm,
                ObjectiveDirection::Maximize,
                'f',
            ),
        ],
        eligibility_rules: vec![
            EligibilityRule {
                id: "correctness_floor".into(),
                metric_id: "correctness_ratio".into(),
                comparator: EligibilityComparator::Gte,
                unit: MetricUnit::RatioPpm,
                threshold: floor,
            },
            EligibilityRule {
                id: "grounding_floor".into(),
                metric_id: "grounding_ratio".into(),
                comparator: EligibilityComparator::Gte,
                unit: MetricUnit::RatioPpm,
                threshold: floor,
            },
        ],
    }
}

fn trial(id: &str, passed: &[&str], failed: &[&str]) -> TrialEvidence {
    TrialEvidence {
        fixture_id: id.into(),
        replicate_index: 0,
        provider_attempt_count: 1,
        outcome: TrialOutcome::Scored,
        passed_scorers: passed.iter().map(|value| (*value).into()).collect(),
        failed_scorers: failed.iter().map(|value| (*value).into()).collect(),
        observations: vec![],
    }
}

fn trial_at(id: &str, replicate_index: u16, passed: bool) -> TrialEvidence {
    let (passed_scorers, failed_scorers) = if passed {
        (vec!["correctness".into(), "grounding".into()], Vec::new())
    } else {
        (Vec::new(), vec!["correctness".into(), "grounding".into()])
    };
    TrialEvidence {
        fixture_id: id.into(),
        replicate_index,
        provider_attempt_count: 1,
        outcome: TrialOutcome::Scored,
        passed_scorers,
        failed_scorers,
        observations: vec![],
    }
}

fn prepared(
    policy: &QualityPolicy,
    fingerprint: char,
    trials: Vec<TrialEvidence>,
) -> PreparedQualityCandidate {
    let candidate_fingerprint = hash(fingerprint);
    PreparedQualityCandidate {
        candidate_fingerprint: Some(candidate_fingerprint.clone()),
        evidence: CandidateEvidence {
            claimed_package_fingerprint: hash('1'),
            claimed_base_contract_fingerprint: hash('2'),
            claimed_fixture_set_fingerprint: hash('3'),
            claimed_candidate_contract_fingerprint: candidate_fingerprint,
            claimed_quality_policy_fingerprint: hash('4'),
            claimed_evaluator_profile_fingerprint: policy
                .claimed_evaluator_profile_fingerprint
                .clone(),
            claimed_model_profile_fingerprint: policy.claimed_model_profile_fingerprint.clone(),
            claimed_scorer_fingerprints: policy
                .binary_scorers
                .iter()
                .map(|item| (item.id.clone(), item.claimed_scorer_fingerprint.clone()))
                .collect(),
            claimed_measurement_profile_fingerprints: policy
                .objectives
                .iter()
                .map(|item| {
                    (
                        item.metric_id.clone(),
                        item.claimed_measurement_profile_fingerprint.clone(),
                    )
                })
                .collect(),
            trials,
        },
        proposal_change_paths: vec!["/messages".into()],
        prevalidation_diagnostics: vec![],
    }
}

type PolicyMutation = Box<dyn Fn(&mut QualityPolicy)>;

#[test]
fn exact_950k_and_850k_boundaries_and_independent_scorer_denominators() {
    for floor in [950_000, 850_000] {
        let policy = policy_two_scorers(floor);
        let validated = validate_quality_policy(&policy).unwrap();
        let fixtures: Vec<_> = (0..20).map(|index| format!("f{index}")).collect();
        let required_passes = usize::try_from(floor / 50_000).unwrap();
        let trials: Vec<_> = fixtures
            .iter()
            .enumerate()
            .map(|(index, fixture)| {
                if index < required_passes {
                    trial(fixture, &["correctness", "grounding"], &[])
                } else {
                    trial(fixture, &[], &["correctness", "grounding"])
                }
            })
            .collect();
        let result =
            assess_quality_candidates(&validated, &fixtures, vec![prepared(&policy, '5', trials)])
                .unwrap();
        let assessment = &result.candidate_assessments[0];
        assert!(
            assessment.eligibility.eligible,
            "{:?}",
            assessment.diagnostics
        );
        assert!(
            assessment
                .aggregates
                .iter()
                .all(|aggregate| aggregate.value == floor)
        );

        let mut below = prepared(
            &policy,
            '6',
            fixtures
                .iter()
                .enumerate()
                .map(|(index, fixture)| {
                    if index + 1 < required_passes {
                        trial(fixture, &["correctness", "grounding"], &[])
                    } else {
                        trial(fixture, &[], &["correctness", "grounding"])
                    }
                })
                .collect(),
        );
        below.evidence.trials[0].passed_scorers = vec!["correctness".into()];
        below.evidence.trials[0].failed_scorers = vec!["grounding".into()];
        let assessment = &assess_quality_candidates(&validated, &fixtures, vec![below])
            .unwrap()
            .candidate_assessments[0];
        assert!(!assessment.eligibility.eligible);
        let correctness = assessment
            .aggregates
            .iter()
            .find(|aggregate| aggregate.metric_id == "correctness_ratio")
            .unwrap()
            .value;
        let grounding = assessment
            .aggregates
            .iter()
            .find(|aggregate| aggregate.metric_id == "grounding_ratio")
            .unwrap()
            .value;
        assert_ne!(correctness, grounding);
    }
}

#[test]
fn policy_requires_exact_nonzero_gte_floor_and_objective_for_every_scorer() {
    let mutations: Vec<PolicyMutation> = vec![
        Box::new(|policy| policy.binary_scorers.clear()),
        Box::new(|policy| policy.eligibility_rules.clear()),
        Box::new(|policy| policy.eligibility_rules[0].threshold = 0),
        Box::new(|policy| policy.eligibility_rules[0].threshold = 1_000_001),
        Box::new(|policy| policy.eligibility_rules[0].comparator = EligibilityComparator::Lte),
        Box::new(|policy| {
            policy
                .eligibility_rules
                .push(policy.eligibility_rules[0].clone())
        }),
        Box::new(|policy| {
            policy.objectives.remove(0);
        }),
        Box::new(|policy| policy.objectives[0].unit = MetricUnit::Milliseconds),
        Box::new(|policy| policy.objectives[0].direction = ObjectiveDirection::Minimize),
    ];
    for mutate in mutations {
        let mut policy = policy_two_scorers(850_000);
        mutate(&mut policy);
        let diagnostics = validate_quality_policy(&policy).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == TQX_QUALITY_POLICY_INVALID)
        );
    }
}

#[test]
fn cardinality_failures_are_ineligible_and_ratios_never_exceed_ppm() {
    let policy = policy_two_scorers(1);
    let validated = validate_quality_policy(&policy).unwrap();
    let fixtures = vec!["a".into(), "b".into()];
    let base = vec![
        trial("a", &["correctness", "grounding"], &[]),
        trial("b", &["correctness", "grounding"], &[]),
    ];
    let variants = [
        vec![base[0].clone()],
        vec![base[0].clone(), base[0].clone()],
        vec![
            base[0].clone(),
            base[1].clone(),
            trial("extra", &["correctness", "grounding"], &[]),
        ],
        vec![
            base[0].clone(),
            TrialEvidence {
                replicate_index: 1,
                ..base[1].clone()
            },
        ],
    ];
    for trials in variants {
        let assessment =
            &assess_quality_candidates(&validated, &fixtures, vec![prepared(&policy, '7', trials)])
                .unwrap()
                .candidate_assessments[0];
        assert!(!assessment.eligibility.eligible);
        assert!(assessment.eligibility.semantic_coverage_ppm <= 1_000_000);
        assert!(assessment.eligibility.infrastructure_failure_ppm <= 1_000_000);
        assert!(
            assessment
                .diagnostics
                .iter()
                .any(|item| item.code == TQX_QUALITY_EVIDENCE_INVALID)
        );
    }
}

fn resource_policy(aggregation: MetricAggregation) -> QualityPolicy {
    let mut policy = policy_two_scorers(1);
    policy.objectives.push(objective(
        "latency",
        "latency_ms",
        MetricUnit::Milliseconds,
        aggregation,
        ObjectiveDirection::Minimize,
        '9',
    ));
    policy
}

fn with_resource(
    mut candidate: PreparedQualityCandidate,
    values: &[u64],
) -> PreparedQualityCandidate {
    candidate
        .evidence
        .claimed_measurement_profile_fingerprints
        .insert("latency_ms".into(), hash('9'));
    for (trial, value) in candidate.evidence.trials.iter_mut().zip(values) {
        trial.observations.push(MetricObservation {
            metric_id: "latency_ms".into(),
            unit: MetricUnit::Milliseconds,
            value: *value,
            claimed_measurement_profile_fingerprint: hash('9'),
            currency_code: None,
            token_kind: None,
        });
    }
    candidate
}

#[test]
fn p95_nearest_rank_vectors_and_checked_sum_overflow() {
    for count in [1_usize, 2, 19, 20, 100] {
        let policy = resource_policy(MetricAggregation::P95NearestRank);
        let validated = validate_quality_policy(&policy).unwrap();
        let fixtures: Vec<_> = (0..count).map(|index| format!("p{index}")).collect();
        let values: Vec<_> = (1..=u64::try_from(count).unwrap()).collect();
        let candidate = with_resource(
            prepared(
                &policy,
                'a',
                fixtures
                    .iter()
                    .map(|id| trial(id, &["correctness", "grounding"], &[]))
                    .collect(),
            ),
            &values,
        );
        let assessment = &assess_quality_candidates(&validated, &fixtures, vec![candidate])
            .unwrap()
            .candidate_assessments[0];
        let p95 = assessment
            .aggregates
            .iter()
            .find(|item| item.metric_id == "latency_ms")
            .unwrap()
            .value;
        assert_eq!(p95, values[(95 * count).div_ceil(100) - 1]);
    }

    let policy = resource_policy(MetricAggregation::Sum);
    let validated = validate_quality_policy(&policy).unwrap();
    let fixtures: Vec<String> = vec!["x".into(), "y".into()];
    let candidate = with_resource(
        prepared(
            &policy,
            'b',
            fixtures
                .iter()
                .map(|id| trial(id, &["correctness", "grounding"], &[]))
                .collect(),
        ),
        &[u64::MAX, 1],
    );
    let assessment = &assess_quality_candidates(&validated, &fixtures, vec![candidate])
        .unwrap()
        .candidate_assessments[0];
    assert!(!assessment.eligibility.eligible);
    assert!(
        assessment
            .diagnostics
            .iter()
            .any(|item| item.code == TQX_QUALITY_LIMIT_EXCEEDED)
    );
}

#[test]
fn infrastructure_has_no_semantic_score_but_counts_against_coverage() {
    let mut policy = policy_two_scorers(1);
    policy.minimum_semantic_cases = 2;
    policy.maximum_infrastructure_failure_ppm = 499_999;
    let validated = validate_quality_policy(&policy).unwrap();
    let fixtures = vec!["ok".into(), "timeout".into()];
    let mut timeout = trial("timeout", &[], &[]);
    timeout.outcome = TrialOutcome::InfrastructureFailure {
        reason: InfrastructureFailureReason::Timeout,
    };
    let candidate = prepared(
        &policy,
        'c',
        vec![trial("ok", &["correctness", "grounding"], &[]), timeout],
    );
    let assessment = &assess_quality_candidates(&validated, &fixtures, vec![candidate])
        .unwrap()
        .candidate_assessments[0];
    assert_eq!(assessment.eligibility.semantic_trial_count, 1);
    assert_eq!(assessment.eligibility.infrastructure_failure_ppm, 500_000);
    assert_eq!(assessment.aggregates[0].value, 1_000_000);
    assert!(!assessment.eligibility.eligible);
}

#[test]
fn candidate_quality_failure_stays_in_each_scorer_denominator() {
    let policy = policy_two_scorers(1);
    let validated = validate_quality_policy(&policy).unwrap();
    let fixtures = vec!["ok".into(), "invalid".into()];
    let mut invalid = trial("invalid", &[], &["correctness", "grounding"]);
    invalid.outcome = TrialOutcome::CandidateQualityFailure {
        reason: CandidateQualityFailureReason::InvalidOutput,
    };
    let assessment = &assess_quality_candidates(
        &validated,
        &fixtures,
        vec![prepared(
            &policy,
            'd',
            vec![trial("ok", &["correctness", "grounding"], &[]), invalid],
        )],
    )
    .unwrap()
    .candidate_assessments[0];
    assert_eq!(assessment.eligibility.semantic_trial_count, 2);
    assert_eq!(assessment.eligibility.infrastructure_trial_count, 0);
    assert!(
        assessment
            .aggregates
            .iter()
            .all(|aggregate| aggregate.value == 500_000)
    );
}

#[test]
fn duplicate_computed_or_claimed_identity_excludes_every_duplicate() {
    let policy = policy_two_scorers(1);
    let validated = validate_quality_policy(&policy).unwrap();
    let fixtures = vec!["only".into()];
    let one = prepared(
        &policy,
        'e',
        vec![trial("only", &["correctness", "grounding"], &[])],
    );
    let mut two = prepared(
        &policy,
        'f',
        vec![trial("only", &["correctness", "grounding"], &[])],
    );
    two.evidence.claimed_candidate_contract_fingerprint =
        one.evidence.claimed_candidate_contract_fingerprint.clone();
    let result = assess_quality_candidates(&validated, &fixtures, vec![one, two]).unwrap();
    assert!(
        result
            .candidate_assessments
            .iter()
            .all(|assessment| !assessment.eligibility.eligible)
    );
    assert!(result.pareto_fronts.is_empty());

    let duplicate = prepared(
        &policy,
        'a',
        vec![trial("only", &["correctness", "grounding"], &[])],
    );
    let result =
        assess_quality_candidates(&validated, &fixtures, vec![duplicate.clone(), duplicate])
            .unwrap();
    assert!(
        result
            .candidate_assessments
            .iter()
            .all(|assessment| !assessment.eligibility.eligible)
    );
}

#[test]
fn eligibility_precedes_deterministic_pareto_and_invalid_identity_stays_claimed() {
    let mut policy = resource_policy(MetricAggregation::Mean);
    policy.eligibility_rules[0].threshold = 1;
    policy.eligibility_rules[1].threshold = 1;
    let validated = validate_quality_policy(&policy).unwrap();
    let fixtures = vec!["a".into(), "b".into()];
    let make = |fingerprint, pass_both, latency| {
        let trials = vec![
            trial("a", &["correctness", "grounding"], &[]),
            if pass_both {
                trial("b", &["correctness", "grounding"], &[])
            } else {
                trial("b", &[], &["correctness", "grounding"])
            },
        ];
        with_resource(prepared(&policy, fingerprint, trials), &[latency, latency])
    };
    let candidates = vec![
        make('d', true, 100),
        make('e', false, 50),
        make('f', false, 100),
    ];
    let result = assess_quality_candidates(&validated, &fixtures, candidates.clone()).unwrap();
    assert_eq!(
        result.pareto_fronts[0].candidate_fingerprints,
        vec![hash('d'), hash('e')]
    );
    assert_eq!(
        result.pareto_fronts[1].candidate_fingerprints,
        vec![hash('f')]
    );
    let mut reversed = candidates;
    reversed.reverse();
    assert_eq!(
        result,
        assess_quality_candidates(&validated, &fixtures, reversed).unwrap()
    );

    let mut invalid = prepared(&policy, '1', vec![]);
    invalid.candidate_fingerprint = None;
    let report = assess_quality_candidates(&validated, &fixtures, vec![invalid]).unwrap();
    assert!(
        report.candidate_assessments[0]
            .candidate_fingerprint
            .is_none()
    );
    assert_eq!(
        report.candidate_assessments[0]
            .claimed_identities
            .as_ref()
            .unwrap()
            .claimed_candidate_contract_fingerprint,
        hash('1')
    );
    assert!(
        report.candidate_assessments[0]
            .diagnostics
            .iter()
            .any(|item| item.code == TQX_QUALITY_CANDIDATE_INVALID)
    );
    assert!(report.pareto_fronts.is_empty());
}

#[test]
fn invalid_candidate_evidence_is_diagnosed_without_reflection() {
    let policy = resource_policy(MetricAggregation::Sum);
    let validated = validate_quality_policy(&policy).unwrap();
    let valid_trial = || trial("only", &["correctness", "grounding"], &[]);

    let mut oversized_value =
        with_resource(prepared(&policy, '2', vec![valid_trial()]), &[u64::MAX]);
    let assessment =
        assess_quality_candidates(&validated, &["only".into()], vec![oversized_value.clone()])
            .unwrap()
            .candidate_assessments
            .remove(0);
    assert!(assessment.trial_summaries.is_empty());
    assert!(assessment.claimed_identities.is_some());
    assert!(
        assessment
            .diagnostics
            .iter()
            .any(|item| item.code == TQX_QUALITY_LIMIT_EXCEEDED)
    );
    assert!(
        !serde_json::to_string(&assessment)
            .unwrap()
            .contains(&u64::MAX.to_string())
    );

    let invalid_ids = [
        ("fixture/customer-secret".to_owned(), "fixture"),
        ("scorer/customer-secret".to_owned(), "scorer"),
        ("metric/customer-secret".to_owned(), "metric"),
        ("x".repeat(129), "fixture"),
        ("y".repeat(129), "scorer"),
        ("z".repeat(129), "metric"),
    ];
    for (invalid_id, location) in invalid_ids {
        let mut candidate = with_resource(prepared(&policy, '3', vec![valid_trial()]), &[1]);
        match location {
            "fixture" => candidate.evidence.trials[0].fixture_id = invalid_id.clone(),
            "scorer" => candidate.evidence.trials[0].passed_scorers[0] = invalid_id.clone(),
            "metric" => candidate.evidence.trials[0].observations[0].metric_id = invalid_id.clone(),
            _ => unreachable!(),
        }
        let assessment = assess_quality_candidates(&validated, &["only".into()], vec![candidate])
            .unwrap()
            .candidate_assessments
            .remove(0);
        let encoded = serde_json::to_string(&assessment).unwrap();
        assert!(assessment.trial_summaries.is_empty(), "{location}");
        assert!(!encoded.contains(&invalid_id), "{location}: {encoded}");
    }

    oversized_value
        .evidence
        .claimed_evaluator_profile_fingerprint = "invalid-customer-profile".into();
    oversized_value.evidence.claimed_scorer_fingerprints.insert(
        "CustomerSecretScorer".into(),
        "invalid-customer-profile".into(),
    );
    let assessment = assess_quality_candidates(&validated, &["only".into()], vec![oversized_value])
        .unwrap()
        .candidate_assessments
        .remove(0);
    let encoded = serde_json::to_string(&assessment).unwrap();
    assert!(assessment.claimed_identities.is_none());
    assert!(!encoded.contains("invalid-customer-profile"));
    assert!(!encoded.contains("CustomerSecretScorer"));

    let mut invalid_observation_profile =
        with_resource(prepared(&policy, '6', vec![valid_trial()]), &[1]);
    invalid_observation_profile.evidence.trials[0].observations[0]
        .claimed_measurement_profile_fingerprint = "customer-observation-profile".into();
    let assessment = assess_quality_candidates(
        &validated,
        &["only".into()],
        vec![invalid_observation_profile],
    )
    .unwrap()
    .candidate_assessments
    .remove(0);
    let encoded = serde_json::to_string(&assessment).unwrap();
    assert!(assessment.trial_summaries.is_empty());
    assert!(!encoded.contains("customer-observation-profile"));
}

#[test]
fn parse_invalid_candidate_preserves_only_protocol_valid_claims() {
    let policy = policy_two_scorers(1);
    let validated = validate_quality_policy(&policy).unwrap();

    let mut valid_claims = prepared(
        &policy,
        '4',
        vec![trial("only", &["correctness", "grounding"], &[])],
    );
    valid_claims.candidate_fingerprint = None;
    let assessment = assess_quality_candidates(&validated, &["only".into()], vec![valid_claims])
        .unwrap()
        .candidate_assessments
        .remove(0);
    assert_eq!(
        assessment
            .claimed_identities
            .as_ref()
            .map(|claims| &claims.claimed_candidate_contract_fingerprint),
        Some(&hash('4'))
    );
    assert_eq!(assessment.trial_summaries.len(), 1);

    let mut invalid_claims = prepared(
        &policy,
        '5',
        vec![trial("only", &["correctness", "grounding"], &[])],
    );
    invalid_claims.candidate_fingerprint = None;
    invalid_claims.evidence.claimed_model_profile_fingerprint = "customer-model-profile".into();
    let assessment = assess_quality_candidates(&validated, &["only".into()], vec![invalid_claims])
        .unwrap()
        .candidate_assessments
        .remove(0);
    assert!(assessment.claimed_identities.is_none());
    assert!(
        !serde_json::to_string(&assessment)
            .unwrap()
            .contains("customer-model-profile")
    );
}

#[test]
fn diagnostic_collector_honors_255_256_257_boundary_after_dedup() {
    for count in [255_usize, 256, 257] {
        let diagnostics: Vec<_> = (0..count)
            .map(|index| Diagnostic {
                code: "TQX_TEST".into(),
                severity: Severity::Error,
                message: "CUSTOMER SECRET".into(),
                file: Some("secret.yaml".into()),
                json_pointer: Some(format!("/{index:03}")),
                span: None,
                help: Some("secret".into()),
            })
            .collect();
        let bounded = bounded_quality_diagnostics(diagnostics);
        assert_eq!(bounded.len(), count.min(256));
        assert!(
            bounded
                .iter()
                .all(|item| item.message == "quality proposal is invalid"
                    && item.file.is_none()
                    && item.help.is_none())
        );
        if count == 257 {
            assert_eq!(
                bounded.last().unwrap().code,
                TQX_QUALITY_DIAGNOSTICS_TRUNCATED
            );
        }
    }

    let duplicate = Diagnostic::error("TQX_DUPLICATE", "first secret", "/same");
    let mut other = duplicate.clone();
    other.message = "second secret".into();
    assert_eq!(bounded_quality_diagnostics(vec![duplicate, other]).len(), 1);
}

#[test]
fn scorer_partition_invalidity_matrix_fails_closed() {
    let policy = policy_two_scorers(1);
    let validated = validate_quality_policy(&policy).unwrap();
    let fixtures = vec!["only".into()];
    let mut variants = Vec::new();

    let mut missing = trial("only", &["correctness"], &[]);
    variants.push(missing.clone());
    missing.failed_scorers = vec!["correctness".into(), "grounding".into()];
    variants.push(missing); // overlap
    let mut duplicate = trial("only", &["correctness", "grounding"], &[]);
    duplicate.passed_scorers.push("correctness".into());
    variants.push(duplicate);
    let mut duplicate_failed = trial("only", &[], &["correctness", "grounding"]);
    duplicate_failed.failed_scorers.push("grounding".into());
    variants.push(duplicate_failed);
    let mut extra = trial("only", &["correctness", "grounding", "unknown"], &[]);
    extra.failed_scorers.clear();
    variants.push(extra);
    variants.push(trial("only", &["correctness", "grounding"], &["unknown"]));
    let mut infrastructure = trial("only", &["correctness", "grounding"], &[]);
    infrastructure.outcome = TrialOutcome::InfrastructureFailure {
        reason: InfrastructureFailureReason::Transport,
    };
    variants.push(infrastructure);
    let mut quality = trial("only", &["correctness", "grounding"], &[]);
    quality.outcome = TrialOutcome::CandidateQualityFailure {
        reason: CandidateQualityFailureReason::Assertion,
    };
    variants.push(quality); // quality failure must fail at least one scorer

    for (index, variant) in variants.into_iter().enumerate() {
        let fingerprint = char::from_digit(u32::try_from(index + 1).unwrap(), 16).unwrap();
        let assessment = &assess_quality_candidates(
            &validated,
            &fixtures,
            vec![prepared(&policy, fingerprint, vec![variant])],
        )
        .unwrap()
        .candidate_assessments[0];
        assert!(!assessment.eligibility.eligible, "variant {index}");
        assert!(
            assessment
                .diagnostics
                .iter()
                .any(|item| item.code == TQX_QUALITY_EVIDENCE_INVALID),
            "variant {index}: {:?}",
            assessment.diagnostics
        );
    }
}

#[test]
fn all_failure_reason_variants_are_partitioned_without_fabricated_scores() {
    let policy = resource_policy(MetricAggregation::Sum);
    let validated = validate_quality_policy(&policy).unwrap();
    let infrastructure_reasons = [
        InfrastructureFailureReason::Transport,
        InfrastructureFailureReason::Timeout,
        InfrastructureFailureReason::RateLimit,
        InfrastructureFailureReason::ProviderUnavailable,
        InfrastructureFailureReason::ProviderInternal,
        InfrastructureFailureReason::Cancellation,
        InfrastructureFailureReason::Budget,
        InfrastructureFailureReason::EvaluatorInfrastructure,
    ];
    for (index, reason) in infrastructure_reasons.into_iter().enumerate() {
        let fixture = format!("infra{index}");
        let mut failed = trial(&fixture, &[], &[]);
        failed.outcome = TrialOutcome::InfrastructureFailure { reason };
        let candidate = with_resource(
            prepared(
                &policy,
                char::from_digit(u32::try_from(index + 1).unwrap(), 16).unwrap(),
                vec![failed],
            ),
            &[u64::try_from(index + 1).unwrap()],
        );
        let assessment =
            &assess_quality_candidates(&validated, std::slice::from_ref(&fixture), vec![candidate])
                .unwrap()
                .candidate_assessments[0];
        assert_eq!(assessment.eligibility.semantic_trial_count, 0);
        assert_eq!(assessment.eligibility.infrastructure_trial_count, 1);
        assert!(
            assessment
                .aggregates
                .iter()
                .all(|item| item.aggregation != MetricAggregation::BinaryRatioPpm)
        );
        assert_eq!(
            assessment.aggregates[0].value,
            u64::try_from(index + 1).unwrap()
        );
        assert!(!assessment.eligibility.eligible);
    }

    for (index, reason) in [
        CandidateQualityFailureReason::Schema,
        CandidateQualityFailureReason::Assertion,
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = format!("quality{index}");
        let mut failed = trial(&fixture, &[], &["correctness", "grounding"]);
        failed.outcome = TrialOutcome::CandidateQualityFailure { reason };
        let assessment = &assess_quality_candidates(
            &validate_quality_policy(&policy_two_scorers(1)).unwrap(),
            std::slice::from_ref(&fixture),
            vec![prepared(&policy_two_scorers(1), 'a', vec![failed])],
        )
        .unwrap()
        .candidate_assessments[0];
        assert_eq!(assessment.eligibility.semantic_trial_count, 1);
        assert!(assessment.aggregates.iter().all(|item| item.value == 0));
    }
}

#[test]
fn mean_sum_and_binary_ratio_use_exact_floor_integer_arithmetic() {
    let fixtures = vec!["a".into(), "b".into(), "c".into()];
    for (aggregation, expected) in [
        (MetricAggregation::Mean, 2_u64),
        (MetricAggregation::Sum, 7_u64),
    ] {
        let policy = resource_policy(aggregation);
        let validated = validate_quality_policy(&policy).unwrap();
        let candidate = with_resource(
            prepared(
                &policy,
                'b',
                vec![
                    trial("a", &["correctness", "grounding"], &[]),
                    trial("b", &[], &["correctness", "grounding"]),
                    trial("c", &[], &["correctness", "grounding"]),
                ],
            ),
            &[1, 2, 4],
        );
        let assessment = &assess_quality_candidates(&validated, &fixtures, vec![candidate])
            .unwrap()
            .candidate_assessments[0];
        assert_eq!(
            assessment
                .aggregates
                .iter()
                .find(|item| item.metric_id == "latency_ms")
                .unwrap()
                .value,
            expected
        );
        assert_eq!(
            assessment
                .aggregates
                .iter()
                .find(|item| item.metric_id == "correctness_ratio")
                .unwrap()
                .value,
            333_333
        );
    }
}

#[test]
fn profile_unit_currency_and_token_comparability_mismatches_are_ineligible() {
    let mut policy = policy_two_scorers(1);
    policy.objectives.extend([
        QualityObjective {
            id: "cost".into(),
            metric_id: "cost".into(),
            unit: MetricUnit::CurrencyMicrounits,
            aggregation: MetricAggregation::Sum,
            direction: ObjectiveDirection::Minimize,
            claimed_measurement_profile_fingerprint: hash('8'),
            currency_code: Some("USD".into()),
            token_kind: None,
        },
        QualityObjective {
            id: "tokens".into(),
            metric_id: "tokens".into(),
            unit: MetricUnit::TokenCount,
            aggregation: MetricAggregation::Sum,
            direction: ObjectiveDirection::Minimize,
            claimed_measurement_profile_fingerprint: hash('9'),
            currency_code: None,
            token_kind: Some(templiqx_contracts::TokenKind::Total),
        },
    ]);
    let validated = validate_quality_policy(&policy).unwrap();
    let base = || {
        let mut candidate = prepared(
            &policy,
            'c',
            vec![trial("only", &["correctness", "grounding"], &[])],
        );
        candidate.evidence.trials[0].observations = vec![
            MetricObservation {
                metric_id: "cost".into(),
                unit: MetricUnit::CurrencyMicrounits,
                value: 1,
                claimed_measurement_profile_fingerprint: hash('8'),
                currency_code: Some("USD".into()),
                token_kind: None,
            },
            MetricObservation {
                metric_id: "tokens".into(),
                unit: MetricUnit::TokenCount,
                value: 2,
                claimed_measurement_profile_fingerprint: hash('9'),
                currency_code: None,
                token_kind: Some(templiqx_contracts::TokenKind::Total),
            },
        ];
        candidate
    };
    type CandidateMutation = Box<dyn Fn(&mut PreparedQualityCandidate)>;
    let mutations: Vec<CandidateMutation> = vec![
        Box::new(|candidate| candidate.evidence.claimed_evaluator_profile_fingerprint = hash('1')),
        Box::new(|candidate| candidate.evidence.claimed_model_profile_fingerprint = hash('1')),
        Box::new(|candidate| {
            candidate
                .evidence
                .claimed_scorer_fingerprints
                .insert("correctness".into(), hash('1'));
        }),
        Box::new(|candidate| {
            candidate
                .evidence
                .claimed_measurement_profile_fingerprints
                .insert("cost".into(), hash('1'));
        }),
        Box::new(|candidate| {
            candidate.evidence.trials[0].observations[0].unit = MetricUnit::Milliseconds
        }),
        Box::new(|candidate| {
            candidate.evidence.trials[0].observations[0].currency_code = Some("EUR".into())
        }),
        Box::new(|candidate| {
            candidate.evidence.trials[0].observations[1].token_kind =
                Some(templiqx_contracts::TokenKind::Prompt)
        }),
        Box::new(|candidate| {
            candidate.evidence.trials[0].observations[0].claimed_measurement_profile_fingerprint =
                hash('1')
        }),
    ];
    for mutate in mutations {
        let mut candidate = base();
        mutate(&mut candidate);
        let assessment = &assess_quality_candidates(&validated, &["only".into()], vec![candidate])
            .unwrap()
            .candidate_assessments[0];
        assert!(!assessment.eligibility.eligible);
    }

    let mut invalid_currency = policy.clone();
    invalid_currency.objectives[2].currency_code = Some("ZZZ".into());
    assert!(validate_quality_policy(&invalid_currency).is_err());
}

#[test]
fn protocol_limits_use_limit_exceeded_and_max_cardinality_is_accepted() {
    let mut too_many_replicates = policy_two_scorers(1);
    too_many_replicates.replicates_per_fixture = 21;
    assert!(
        validate_quality_policy(&too_many_replicates)
            .unwrap_err()
            .iter()
            .any(|item| item.code == templiqx_contracts::TQX_QUALITY_LIMIT_EXCEEDED)
    );
    let mut too_long_id = policy_two_scorers(1);
    too_long_id.id = "a".repeat(129);
    assert!(
        validate_quality_policy(&too_long_id)
            .unwrap_err()
            .iter()
            .any(|item| item.code == templiqx_contracts::TQX_QUALITY_LIMIT_EXCEEDED)
    );
    let mut max_id = policy_two_scorers(1);
    max_id.id = "a".repeat(128);
    assert!(validate_quality_policy(&max_id).is_ok());

    let many_scorers = |count: usize| QualityPolicy {
        id: "many".into(),
        replicates_per_fixture: 1,
        minimum_semantic_cases: 1,
        maximum_infrastructure_failure_ppm: 0,
        claimed_evaluator_profile_fingerprint: hash('a'),
        claimed_model_profile_fingerprint: hash('b'),
        binary_scorers: (0..count)
            .map(|index| scorer(&format!("s{index}"), &format!("m{index}"), 'c'))
            .collect(),
        objectives: (0..count)
            .map(|index| {
                objective(
                    &format!("s{index}"),
                    &format!("m{index}"),
                    MetricUnit::RatioPpm,
                    MetricAggregation::BinaryRatioPpm,
                    ObjectiveDirection::Maximize,
                    'd',
                )
            })
            .collect(),
        eligibility_rules: (0..count)
            .map(|index| EligibilityRule {
                id: format!("floor{index}"),
                metric_id: format!("m{index}"),
                comparator: EligibilityComparator::Gte,
                unit: MetricUnit::RatioPpm,
                threshold: 1,
            })
            .collect(),
    };
    assert!(validate_quality_policy(&many_scorers(16)).is_ok());
    assert!(
        validate_quality_policy(&many_scorers(17))
            .unwrap_err()
            .iter()
            .any(|item| item.code == templiqx_contracts::TQX_QUALITY_LIMIT_EXCEEDED)
    );

    let mut policy = policy_two_scorers(1);
    policy.replicates_per_fixture = 20;
    let validated = validate_quality_policy(&policy).unwrap();
    let fixtures: Vec<_> = (0..512).map(|index| format!("fixture{index}")).collect();
    let mut too_many_fixtures = fixtures.clone();
    too_many_fixtures.push("fixture512".into());
    assert!(
        assess_quality_candidates(&validated, &too_many_fixtures, vec![])
            .unwrap_err()
            .iter()
            .any(|item| item.code == templiqx_contracts::TQX_QUALITY_LIMIT_EXCEEDED)
    );
    let trials: Vec<_> = fixtures
        .iter()
        .flat_map(|fixture| (0..20).map(move |replicate| trial_at(fixture, replicate, true)))
        .collect();
    assert_eq!(trials.len(), 10_240);
    let valid = assess_quality_candidates(
        &validated,
        &fixtures,
        vec![prepared(&policy, 'd', trials.clone())],
    )
    .unwrap();
    assert!(valid.candidate_assessments[0].eligibility.eligible);

    let mut over = trials;
    over.push(trial_at(&fixtures[0], 0, true));
    let assessment =
        &assess_quality_candidates(&validated, &fixtures, vec![prepared(&policy, 'e', over)])
            .unwrap()
            .candidate_assessments[0];
    assert!(
        assessment
            .diagnostics
            .iter()
            .any(|item| item.code == templiqx_contracts::TQX_QUALITY_LIMIT_EXCEEDED)
    );

    let small_policy = policy_two_scorers(1);
    let small_validated = validate_quality_policy(&small_policy).unwrap();
    let mut excessive_observations = prepared(
        &small_policy,
        'f',
        vec![trial("only", &["correctness", "grounding"], &[])],
    );
    excessive_observations.evidence.trials[0].observations = (0..17)
        .map(|index| MetricObservation {
            metric_id: format!("metric{index}"),
            unit: MetricUnit::Milliseconds,
            value: 1,
            claimed_measurement_profile_fingerprint: hash('1'),
            currency_code: None,
            token_kind: None,
        })
        .collect();
    let assessment = &assess_quality_candidates(
        &small_validated,
        &["only".into()],
        vec![excessive_observations],
    )
    .unwrap()
    .candidate_assessments[0];
    assert!(
        assessment
            .diagnostics
            .iter()
            .any(|item| item.code == templiqx_contracts::TQX_QUALITY_LIMIT_EXCEEDED)
    );

    let mut overlong_evidence_id = prepared(
        &small_policy,
        '1',
        vec![trial("only", &["correctness", "grounding"], &[])],
    );
    overlong_evidence_id.evidence.trials[0].passed_scorers = vec!["x".repeat(129)];
    let assessment = &assess_quality_candidates(
        &small_validated,
        &["only".into()],
        vec![overlong_evidence_id],
    )
    .unwrap()
    .candidate_assessments[0];
    assert!(
        assessment
            .diagnostics
            .iter()
            .any(|item| item.code == templiqx_contracts::TQX_QUALITY_LIMIT_EXCEEDED)
    );
}

#[test]
fn max_cardinality_reproduces_exact_floors_and_rejects_one_fewer_pass() {
    let mut policy = policy_two_scorers(950_000);
    policy.replicates_per_fixture = 20;
    let fixtures: Vec<_> = (0..512).map(|index| format!("f{index}")).collect();
    for (floor, exact_passes, below_passes, closest_below) in [
        (950_000, 9_728_usize, 9_727_usize, 949_902_u64),
        (850_000, 8_704_usize, 8_703_usize, 849_902_u64),
    ] {
        policy.eligibility_rules[0].threshold = floor;
        policy.eligibility_rules[1].threshold = floor;
        let validated = validate_quality_policy(&policy).unwrap();
        let build = |passes: usize, fingerprint| {
            let trials = fixtures
                .iter()
                .flat_map(|fixture| (0..20).map(move |replicate| (fixture, replicate)))
                .enumerate()
                .map(|(index, (fixture, replicate))| trial_at(fixture, replicate, index < passes))
                .collect();
            prepared(&policy, fingerprint, trials)
        };
        let exact =
            assess_quality_candidates(&validated, &fixtures, vec![build(exact_passes, 'a')])
                .unwrap();
        assert!(exact.candidate_assessments[0].eligibility.eligible);
        assert_eq!(exact.candidate_assessments[0].aggregates[0].value, floor);
        let below =
            assess_quality_candidates(&validated, &fixtures, vec![build(below_passes, 'b')])
                .unwrap();
        assert!(!below.candidate_assessments[0].eligibility.eligible);
        assert_eq!(
            below.candidate_assessments[0].aggregates[0].value,
            closest_below
        );
    }
}

#[test]
fn closest_protocol_representable_values_below_reference_floors_are_rejected() {
    // With fixtures <= 512 and one fixed replicate count <= 20, 949_999 and
    // 849_999 are not representable as floor(passed * 1_000_000 / semantic).
    // Exhausting that bounded denominator grid yields these closest values.
    for (floor, fixture_count, passes, closest_below) in [
        (950_000, 507_usize, 8_188_usize, 949_994_u64),
        (850_000, 509_usize, 7_355_usize, 849_994_u64),
    ] {
        let mut policy = policy_two_scorers(floor);
        policy.replicates_per_fixture = 17;
        let validated = validate_quality_policy(&policy).unwrap();
        let fixtures: Vec<_> = (0..fixture_count)
            .map(|index| format!("nearest{index}"))
            .collect();
        let trials = fixtures
            .iter()
            .flat_map(|fixture| (0..17).map(move |replicate| (fixture, replicate)))
            .enumerate()
            .map(|(index, (fixture, replicate))| trial_at(fixture, replicate, index < passes))
            .collect();
        let assessment =
            &assess_quality_candidates(&validated, &fixtures, vec![prepared(&policy, 'f', trials)])
                .unwrap()
                .candidate_assessments[0];
        assert_eq!(assessment.aggregates[0].value, closest_below);
        assert!(!assessment.eligibility.eligible);
    }
}

#[test]
fn identical_metric_candidates_share_the_same_pareto_front() {
    let policy = resource_policy(MetricAggregation::Mean);
    let validated = validate_quality_policy(&policy).unwrap();
    let candidate = |fingerprint| {
        with_resource(
            prepared(
                &policy,
                fingerprint,
                vec![trial("only", &["correctness", "grounding"], &[])],
            ),
            &[42],
        )
    };
    let result = assess_quality_candidates(
        &validated,
        &["only".into()],
        vec![candidate('c'), candidate('d')],
    )
    .unwrap();
    assert_eq!(result.pareto_fronts.len(), 1);
    assert_eq!(
        result.pareto_fronts[0].candidate_fingerprints,
        vec![hash('c'), hash('d')]
    );
}

#[test]
fn invalid_duplicate_tie_keys_are_reported_permutation_invariantly() {
    let policy = resource_policy(MetricAggregation::Sum);
    let validated = validate_quality_policy(&policy).unwrap();
    let mut first = with_resource(
        prepared(
            &policy,
            'e',
            vec![
                trial("only", &["correctness", "grounding"], &[]),
                trial("only", &["correctness", "grounding"], &[]),
            ],
        ),
        &[2, 1],
    );
    first.evidence.trials[1].provider_attempt_count = 2;
    let mut reversed = first.clone();
    reversed.evidence.trials.reverse();
    for trial in &mut reversed.evidence.trials {
        trial.observations.reverse();
    }
    assert_eq!(
        assess_quality_candidates(&validated, &["only".into()], vec![first]).unwrap(),
        assess_quality_candidates(&validated, &["only".into()], vec![reversed]).unwrap()
    );
}
