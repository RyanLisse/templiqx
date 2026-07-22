use std::collections::BTreeMap;

use templiqx_contracts::*;

fn hash(ch: char) -> String {
    std::iter::repeat_n(ch, 64).collect()
}

fn policy() -> QualityPolicy {
    QualityPolicy {
        id: "reference".into(),
        replicates_per_fixture: 2,
        minimum_semantic_cases: 1,
        maximum_infrastructure_failure_ppm: 100_000,
        claimed_evaluator_profile_fingerprint: hash('a'),
        claimed_model_profile_fingerprint: hash('b'),
        binary_scorers: vec![BinaryScorer {
            id: "correctness".into(),
            metric_id: "correctness_ratio".into(),
            claimed_scorer_fingerprint: hash('c'),
        }],
        objectives: vec![QualityObjective {
            id: "correctness".into(),
            metric_id: "correctness_ratio".into(),
            unit: MetricUnit::RatioPpm,
            aggregation: MetricAggregation::BinaryRatioPpm,
            direction: ObjectiveDirection::Maximize,
            claimed_measurement_profile_fingerprint: hash('d'),
            currency_code: None,
            token_kind: None,
        }],
        eligibility_rules: vec![EligibilityRule {
            id: "correctness_floor".into(),
            metric_id: "correctness_ratio".into(),
            comparator: EligibilityComparator::Gte,
            unit: MetricUnit::RatioPpm,
            threshold: 950_000,
        }],
    }
}

fn evidence() -> CandidateEvidence {
    CandidateEvidence {
        claimed_package_fingerprint: hash('1'),
        claimed_base_contract_fingerprint: hash('2'),
        claimed_fixture_set_fingerprint: hash('3'),
        claimed_candidate_contract_fingerprint: hash('4'),
        claimed_quality_policy_fingerprint: hash('5'),
        claimed_evaluator_profile_fingerprint: hash('a'),
        claimed_model_profile_fingerprint: hash('b'),
        claimed_scorer_fingerprints: BTreeMap::from([("correctness".into(), hash('c'))]),
        claimed_measurement_profile_fingerprints: BTreeMap::from([(
            "correctness_ratio".into(),
            hash('d'),
        )]),
        trials: vec![TrialEvidence {
            fixture_id: "case-a".into(),
            replicate_index: 0,
            provider_attempt_count: 1,
            outcome: TrialOutcome::Scored,
            passed_scorers: vec!["correctness".into()],
            failed_scorers: vec![],
            observations: vec![],
        }],
    }
}

#[test]
fn quality_wire_types_round_trip_and_reject_unknowns_and_floats() {
    let request = QualityProposalRequest {
        package: "demo".into(),
        contract_id: "greeting".into(),
        expected_package_fingerprint: hash('1'),
        expected_base_contract_fingerprint: hash('2'),
        expected_fixture_set_fingerprint: hash('3'),
        policy: policy(),
        candidates: vec![QualityCandidateSubmission {
            candidate_source: "api_version: templiqx/v1alpha1".into(),
            synthetic_or_sanitized_data_attestation: true,
            evidence: evidence(),
        }],
    };
    let json = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        serde_json::from_slice::<QualityProposalRequest>(&json).unwrap(),
        request
    );
    let yaml = serde_yaml_ng::to_string(&request).unwrap();
    assert_eq!(
        serde_yaml_ng::from_str::<QualityProposalRequest>(&yaml).unwrap(),
        request
    );

    let mut unknown = serde_json::to_value(&request).unwrap();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("raw_output".into(), serde_json::json!("secret"));
    assert!(serde_json::from_value::<QualityProposalRequest>(unknown).is_err());

    let mut float = serde_json::to_value(&request).unwrap();
    float["policy"]["minimum_semantic_cases"] = serde_json::json!(1.5);
    assert!(serde_json::from_value::<QualityProposalRequest>(float).is_err());
}

#[test]
fn all_fingerprint_domains_have_frozen_golden_vectors() {
    let identity = PackageIdentity {
        manifest: PackageManifest {
            api_version: "templiqx.package/v1alpha1".into(),
            package: "demo".into(),
            version: "1.0.0".into(),
            description: String::new(),
            contracts: vec!["greeting".into()],
            components: vec![],
            evals: vec![],
            migrations: vec![],
            templates: vec![],
            provenance: BTreeMap::new(),
            signatures: vec![],
            dependencies: BTreeMap::new(),
            tool_contracts: BTreeMap::new(),
            translations: vec![],
            definitions: vec![],
        },
        artifacts: BTreeMap::from([("contracts/greeting.yaml".into(), hash('6'))]),
    };
    let fixtures = vec![EvalFixture {
        id: "case-a".into(),
        inputs: BTreeMap::from([("name".into(), serde_json::json!("Ada"))]),
        context: BTreeMap::new(),
        fake_output: serde_json::json!({"greeting":"Hello Ada"}),
    }];
    let policy = policy();
    let request = QualityRequestFingerprintPayload {
        package: "demo".into(),
        contract_id: "greeting".into(),
        expected_package_fingerprint: hash('1'),
        expected_base_contract_fingerprint: hash('2'),
        expected_fixture_set_fingerprint: hash('3'),
        policy: policy.clone(),
        candidates: vec![QualityCandidateFingerprintPayload {
            candidate_contract_fingerprint: Some(hash('4')),
            synthetic_or_sanitized_data_attestation: true,
            evidence: evidence(),
        }],
    };
    let contract: Contract = serde_yaml_ng::from_str(include_str!(
        "../../../examples/packages/demo/contracts/greeting.yaml"
    ))
    .unwrap();
    let computed = ComputedQualityIdentities {
        package_fingerprint: hash('1'),
        base_contract_fingerprint: hash('2'),
        fixture_set_fingerprint: hash('3'),
        quality_policy_fingerprint: hash('5'),
        request_fingerprint: hash('6'),
    };
    let report = QualityProposalReportPayload {
        computed_identities: computed,
        candidate_assessments: vec![],
        pareto_fronts: vec![],
    };

    let actual = [
        quality_package_fingerprint(&identity).unwrap(),
        fingerprint(&contract).unwrap(),
        quality_fixture_set_fingerprint(&fixtures).unwrap(),
        quality_policy_fingerprint(&policy).unwrap(),
        quality_request_fingerprint(&request).unwrap(),
        quality_report_fingerprint(&report).unwrap(),
    ];
    assert_eq!(
        actual,
        [
            "ec65076ec0c183ae4a87967184cd1637be042a9a6281142d9228017b21cb502f",
            "4115586b3ca64e8fcc73dcd5b90f3623b4325b0b56783db4192557772bb2ff4c",
            "bc04de60ac9a2abe1361fd9ae221816e6422a22de49b3a2e33bd5f9f01c49574",
            "9aed52cc3d601928150f804158951e89c9aae3130f81571fce6cc1bce292a3da",
            "0b6f164a39589bb2d0ef68e4e7ebfb08b7735341cc8a301b328f700d82f3f923",
            "420eed38f25f7fae4ea4ff1f79fddca645ac6a23ae0f96af75e384f4e519bd1f",
        ]
    );
}

#[test]
fn normalization_is_permutation_invariant_and_parse_invalid_identity_is_optional() {
    let mut left = evidence();
    left.trials.push(TrialEvidence {
        fixture_id: "case-b".into(),
        replicate_index: 0,
        provider_attempt_count: 2,
        outcome: TrialOutcome::CandidateQualityFailure {
            reason: CandidateQualityFailureReason::Assertion,
        },
        passed_scorers: vec![],
        failed_scorers: vec!["correctness".into()],
        observations: vec![],
    });
    let mut right = left.clone();
    right.trials.reverse();
    assert_eq!(left.normalized(), right.normalized());

    let invalid = QualityCandidateFingerprintPayload {
        candidate_contract_fingerprint: None,
        synthetic_or_sanitized_data_attestation: false,
        evidence: left,
    };
    let encoded = serde_json::to_value(&invalid).unwrap();
    assert!(encoded.get("candidate_contract_fingerprint").is_none());
    assert_eq!(
        serde_json::from_value::<QualityCandidateFingerprintPayload>(encoded)
            .unwrap()
            .candidate_contract_fingerprint,
        None
    );
}

#[test]
fn invalid_claimed_identities_can_be_omitted_from_candidate_reports() {
    let assessment = CandidateAssessment {
        candidate_fingerprint: None,
        claimed_identities: None,
        eligibility: EligibilityAssessment {
            eligible: false,
            total_trial_count: 1,
            semantic_trial_count: 0,
            infrastructure_trial_count: 0,
            semantic_coverage_ppm: 0,
            infrastructure_failure_ppm: 0,
            gates: vec![],
        },
        aggregates: vec![],
        trial_summaries: vec![],
        proposal_change_paths: vec![],
        diagnostics: vec![],
    };
    let encoded = serde_json::to_value(&assessment).unwrap();
    assert!(encoded.get("claimed_identities").is_none());
    assert_eq!(
        serde_json::from_value::<CandidateAssessment>(encoded).unwrap(),
        assessment
    );
}

#[test]
fn duplicate_trial_and_observation_tie_keys_have_total_canonical_order() {
    let mut first = evidence();
    let mut duplicate = first.trials[0].clone();
    duplicate.provider_attempt_count = 2;
    duplicate.outcome = TrialOutcome::CandidateQualityFailure {
        reason: CandidateQualityFailureReason::Schema,
    };
    duplicate.passed_scorers.clear();
    duplicate.failed_scorers = vec!["correctness".into()];
    let observation_a = MetricObservation {
        metric_id: "cost".into(),
        unit: MetricUnit::CurrencyMicrounits,
        value: 2,
        claimed_measurement_profile_fingerprint: hash('e'),
        currency_code: Some("EUR".into()),
        token_kind: None,
    };
    let mut observation_b = observation_a.clone();
    observation_b.value = 1;
    first.trials[0].observations = vec![observation_a, observation_b];
    first.trials.push(duplicate);

    let mut reversed = first.clone();
    reversed.trials.reverse();
    for trial in &mut reversed.trials {
        trial.observations.reverse();
    }
    assert_eq!(first.normalized(), reversed.normalized());

    let payload = |evidence| QualityRequestFingerprintPayload {
        package: "demo".into(),
        contract_id: "greeting".into(),
        expected_package_fingerprint: hash('1'),
        expected_base_contract_fingerprint: hash('2'),
        expected_fixture_set_fingerprint: hash('3'),
        policy: policy(),
        candidates: vec![QualityCandidateFingerprintPayload {
            candidate_contract_fingerprint: Some(hash('4')),
            synthetic_or_sanitized_data_attestation: true,
            evidence,
        }],
    };
    let attested = payload(first);
    assert_eq!(
        quality_request_fingerprint(&attested).unwrap(),
        quality_request_fingerprint(&payload(reversed)).unwrap()
    );
    let mut unattested = attested.clone();
    unattested.candidates[0].synthetic_or_sanitized_data_attestation = false;
    assert_ne!(
        quality_request_fingerprint(&attested).unwrap(),
        quality_request_fingerprint(&unattested).unwrap()
    );
}
