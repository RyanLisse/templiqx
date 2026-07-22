use std::time::Duration;

use templiqx_adapter_rust::{
    CallOptions, Client, ClientOptions, TempliqxError,
    generated::{
        CandidateAssessment, ClaimedQualityIdentities, MetricObservation, QualityProposalRequest,
    },
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn client(server: &MockServer, timeout: Duration) -> Client {
    Client::new(
        format!("{}/", server.uri()),
        ClientOptions {
            timeout,
            ..ClientOptions::default()
        },
    )
    .expect("client")
}

fn envelope(operation: &str, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "api_version": "templiqx/v1alpha1",
        "diagnostics": [],
        "fingerprints": {},
        "ok": true,
        "operation": operation,
        "result": result,
        "stream_events": []
    })
}

#[tokio::test]
async fn encodes_artifact_path_query_and_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operations/v1/artifacts/folder/a%20b.json"))
        .and(query_param("package", "sdk package"))
        .and(query_param("workspace", "review"))
        .and(header("x-request-id", "unit-request"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-request-id", "server-request")
                .set_body_json(envelope(
                    "read_artifact",
                    serde_json::json!({"bytes": "abc"}),
                )),
        )
        .mount(&server)
        .await;

    let response = client(&server, Duration::from_secs(2))
        .read_artifact(
            "folder/a b.json",
            "sdk package",
            Some("review"),
            CallOptions {
                request_id: Some("unit-request".into()),
                ..CallOptions::default()
            },
        )
        .await
        .expect("response");

    assert!(response.data.ok);
    assert_eq!(response.request_id, "server-request");
}

#[tokio::test]
async fn posts_typed_quality_evidence_to_the_package_scoped_route() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/operations/v1/packages/demo%20package/quality/proposals:assess",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(
            "assess_quality_proposals",
            serde_json::Value::Null,
        )))
        .mount(&server)
        .await;

    let fingerprint = "a".repeat(64);
    let request: QualityProposalRequest = serde_json::from_value(serde_json::json!({
        "package": "demo package",
        "contract_id": "greeting",
        "expected_package_fingerprint": fingerprint,
        "expected_base_contract_fingerprint": fingerprint,
        "expected_fixture_set_fingerprint": fingerprint,
        "policy": {
            "id": "quality-policy",
            "replicates_per_fixture": 1,
            "minimum_semantic_cases": 1,
            "maximum_infrastructure_failure_ppm": 0,
            "claimed_evaluator_profile_fingerprint": fingerprint,
            "claimed_model_profile_fingerprint": fingerprint,
            "binary_scorers": [],
            "objectives": [],
            "eligibility_rules": []
        },
        "candidates": []
    }))
    .expect("typed quality request");

    let response = client(&server, Duration::from_secs(2))
        .assess_quality_proposals("demo package", &request, CallOptions::default())
        .await
        .expect("response");

    assert_eq!(response.data.operation, "assess_quality_proposals");
}

#[test]
fn generated_quality_integers_round_trip_at_the_public_ceiling_and_claims_stay_explicit() {
    let fingerprint = "a".repeat(64);
    let observation: MetricObservation = serde_json::from_value(serde_json::json!({
        "metric_id": "total_tokens",
        "unit": "token_count",
        "value": 9_007_199_254_740_991_i64,
        "claimed_measurement_profile_fingerprint": fingerprint,
        "token_kind": "total"
    }))
    .expect("maximum public integer should deserialize");
    assert_eq!(observation.value, 9_007_199_254_740_991_i64);

    let identities: ClaimedQualityIdentities = serde_json::from_value(serde_json::json!({
        "claimed_candidate_contract_fingerprint": fingerprint,
        "claimed_evaluator_profile_fingerprint": fingerprint,
        "claimed_model_profile_fingerprint": fingerprint,
        "claimed_scorer_fingerprints": {"grounded": fingerprint},
        "claimed_measurement_profile_fingerprints": {"total_tokens": fingerprint}
    }))
    .expect("explicit claimed identity fields should deserialize");
    let encoded = serde_json::to_value(identities).expect("claimed identities should serialize");
    assert!(
        encoded
            .get("claimed_candidate_contract_fingerprint")
            .is_some()
    );
    assert!(encoded.get("candidate_contract_fingerprint").is_none());

    let invalid_assessment: CandidateAssessment = serde_json::from_value(serde_json::json!({
        "eligibility": {
            "eligible": false,
            "total_trial_count": 0,
            "semantic_trial_count": 0,
            "infrastructure_trial_count": 0,
            "semantic_coverage_ppm": 0,
            "infrastructure_failure_ppm": 0,
            "gates": []
        },
        "aggregates": [],
        "trial_summaries": [],
        "proposal_change_paths": [],
        "diagnostics": []
    }))
    .expect("assessment without valid claims should deserialize");
    assert!(invalid_assessment.claimed_identities.is_none());
    let encoded = serde_json::to_value(invalid_assessment)
        .expect("assessment without valid claims should serialize");
    assert!(encoded.get("claimed_identities").is_none());
}

#[tokio::test]
async fn per_call_timeout_maps_to_transport_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operations/v1/catalog"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(100))
                .set_body_json(envelope("catalog", serde_json::json!([]))),
        )
        .mount(&server)
        .await;

    let error = client(&server, Duration::from_secs(2))
        .catalog(CallOptions {
            timeout: Some(Duration::from_millis(5)),
            request_id: Some("timeout-request".into()),
            ..CallOptions::default()
        })
        .await
        .expect_err("request should time out");

    match error {
        TempliqxError::Transport(error) => {
            assert_eq!(error.request_id, "timeout-request");
            assert!(error.is_timeout());
        }
        other => panic!("expected transport error, got {other:?}"),
    }
}

#[tokio::test]
async fn non_success_json_maps_diagnostics_and_text_keeps_raw_body() {
    let server = MockServer::start().await;
    let diagnostics = serde_json::json!({
        "api_version": "templiqx/v1alpha1",
        "diagnostics": [{
            "code": "TQX_NOT_FOUND",
            "message": "not found",
            "severity": "error"
        }],
        "fingerprints": {},
        "ok": false,
        "operation": "catalog",
        "stream_events": []
    });
    Mock::given(method("GET"))
        .and(path("/operations/v1/catalog"))
        .respond_with(ResponseTemplate::new(404).set_body_json(diagnostics))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/operations/v1/catalog"))
        .respond_with(ResponseTemplate::new(502).set_body_string("bad gateway"))
        .mount(&server)
        .await;

    let first = client(&server, Duration::from_secs(2))
        .catalog(CallOptions::default())
        .await
        .expect_err("404");
    match first {
        TempliqxError::Http(error) => {
            assert_eq!(error.status.as_u16(), 404);
            assert_eq!(
                error.envelope.expect("envelope").diagnostics[0].code,
                "TQX_NOT_FOUND"
            );
            assert!(error.raw_body.is_none());
        }
        other => panic!("expected HTTP error, got {other:?}"),
    }

    let second = client(&server, Duration::from_secs(2))
        .catalog(CallOptions::default())
        .await
        .expect_err("502");
    match second {
        TempliqxError::Http(error) => {
            assert_eq!(error.status.as_u16(), 502);
            assert!(error.envelope.is_none());
            assert_eq!(error.raw_body.as_deref(), Some("bad gateway"));
        }
        other => panic!("expected HTTP error, got {other:?}"),
    }
}

#[tokio::test]
async fn default_request_id_is_a_version_four_uuid() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operations/v1/catalog"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(envelope("catalog", serde_json::json!([]))),
        )
        .mount(&server)
        .await;

    let response = client(&server, Duration::from_secs(2))
        .catalog(CallOptions::default())
        .await
        .expect("response");
    let bytes = response.request_id.as_bytes();
    assert_eq!(response.request_id.len(), 36);
    assert_eq!(bytes[14], b'4');
    assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'));
}
