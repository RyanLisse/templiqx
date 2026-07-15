"""Smoke coverage for the synchronous Templiqx Operations API client."""

import httpx

from templiqx_adapter import ExecuteRequest, JsonValue, TempliqxClient
from templiqx_adapter.compat import assert_compatibility


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
