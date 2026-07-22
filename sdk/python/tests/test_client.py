"""Smoke coverage for the synchronous Templiqx Operations API client."""

import json

import httpx
import pytest
from pydantic import ValidationError
from templiqx_adapter._generated.operations_v1 import CandidateAssessment

from templiqx_adapter import ExecuteRequest, JsonValue, QualityProposalRequest, TempliqxClient
from templiqx_adapter.compat import assert_compatibility, compatibility


def test_execute_contract_round_trip() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "POST"
        assert request.url.path == "/operations/v1/packages/demo/contracts/welcome/execute"
        assert request.read() == b'{"fixture_output":{"greeting":"hello"}}'
        return httpx.Response(
            200,
            headers={"x-request-id": "request-from-server"},
            json={
                "api_version": "templiqx/v1alpha1",
                "operation": "execute_contract",
                "ok": True,
                "diagnostics": [],
                "fingerprints": {
                    "request": "sha256:request",
                    "output": "sha256:output",
                },
                "result": {
                    "adapter": {
                        "id": "stub",
                        "version": "1.0.0",
                        "capabilities": [],
                    },
                    "request_fingerprint": "sha256:request",
                    "output_fingerprint": "sha256:output",
                    "output": {"greeting": "hello"},
                    "output_schema_valid": True,
                },
            },
        )

    assert_compatibility()
    assert compatibility.engine_api_version == "0.2"
    assert compatibility.engine_version == "0.2.0"
    assert compatibility.contract_format == "templiqx/v1alpha1"
    with httpx.Client(transport=httpx.MockTransport(handler)) as transport:
        response = TempliqxClient("https://templiqx.example", client=transport).execute_contract(
            "demo",
            "welcome",
            ExecuteRequest(fixture_output=JsonValue({"greeting": "hello"})),
        )

    assert response.request_id == "request-from-server"
    assert response.data.ok is True
    assert response.data.operation == "execute_contract"
    assert response.data.result is not None
    assert response.data.result.output.root == {"greeting": "hello"}


def test_assess_quality_proposals_uses_typed_package_scoped_route() -> None:
    fingerprint = "a" * 64
    request_body = QualityProposalRequest.model_validate(
        {
            "package": "demo package",
            "contract_id": "welcome",
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
                "binary_scorers": [
                    {
                        "id": "grounded",
                        "metric_id": "grounded_ppm",
                        "claimed_scorer_fingerprint": fingerprint,
                    }
                ],
                "objectives": [
                    {
                        "id": "grounded",
                        "metric_id": "grounded_ppm",
                        "unit": "ratio_ppm",
                        "aggregation": "binary_ratio_ppm",
                        "direction": "maximize",
                        "claimed_measurement_profile_fingerprint": fingerprint,
                    }
                ],
                "eligibility_rules": [
                    {
                        "id": "grounded-floor",
                        "metric_id": "grounded_ppm",
                        "comparator": "gte",
                        "unit": "ratio_ppm",
                        "threshold": 850_000,
                    }
                ],
            },
            "candidates": [
                {
                    "candidate_source": "api_version: templiqx/v1alpha1",
                    "synthetic_or_sanitized_data_attestation": True,
                    "evidence": {
                        "claimed_package_fingerprint": fingerprint,
                        "claimed_base_contract_fingerprint": fingerprint,
                        "claimed_fixture_set_fingerprint": fingerprint,
                        "claimed_candidate_contract_fingerprint": fingerprint,
                        "claimed_quality_policy_fingerprint": fingerprint,
                        "claimed_evaluator_profile_fingerprint": fingerprint,
                        "claimed_model_profile_fingerprint": fingerprint,
                        "claimed_scorer_fingerprints": {"grounded": fingerprint},
                        "claimed_measurement_profile_fingerprints": {
                            "grounded_ppm": fingerprint
                        },
                        "trials": [
                            {
                                "fixture_id": "fixture-1",
                                "replicate_index": 0,
                                "provider_attempt_count": 1,
                                "outcome": {"kind": "scored"},
                                "passed_scorers": ["grounded"],
                                "failed_scorers": [],
                                "observations": [],
                            }
                        ],
                    },
                }
            ],
        }
    )

    at_limit = request_body.model_dump(mode="json")
    at_limit["policy"]["minimum_semantic_cases"] = 9_007_199_254_740_991
    assert (
        QualityProposalRequest.model_validate(at_limit).policy.minimum_semantic_cases
        == 9_007_199_254_740_991
    )
    at_limit["policy"]["minimum_semantic_cases"] = 9_007_199_254_740_992
    with pytest.raises(ValidationError):
        QualityProposalRequest.model_validate(at_limit)

    invalid_assessment = CandidateAssessment.model_validate(
        {
            "eligibility": {
                "eligible": False,
                "total_trial_count": 0,
                "semantic_trial_count": 0,
                "infrastructure_trial_count": 0,
                "semantic_coverage_ppm": 0,
                "infrastructure_failure_ppm": 0,
                "gates": [],
            },
            "aggregates": [],
            "trial_summaries": [],
            "proposal_change_paths": [],
            "diagnostics": [],
        }
    )
    assert invalid_assessment.claimed_identities is None
    assert "claimed_identities" not in invalid_assessment.model_dump(exclude_none=True)

    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "POST"
        assert (
            request.url.raw_path
            == b"/operations/v1/packages/demo%20package/quality/proposals:assess"
        )
        assert json.loads(request.read())["contract_id"] == "welcome"
        return httpx.Response(
            200,
            json={
                "api_version": "templiqx/v1alpha1",
                "operation": "assess_quality_proposals",
                "ok": True,
                "diagnostics": [],
                "fingerprints": {},
            },
        )

    with httpx.Client(transport=httpx.MockTransport(handler)) as transport:
        response = TempliqxClient(
            "https://templiqx.example", client=transport
        ).assess_quality_proposals("demo package", request_body)

    assert response.data.operation == "assess_quality_proposals"
