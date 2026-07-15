"""Generated-contract compatibility metadata for the Python SDK."""

from __future__ import annotations

from dataclasses import dataclass

from ._generated.operations_v1 import (
    GENERATED_OPENAPI_DIGEST,
    GENERATED_OPENAPI_VERSION,
    GENERATED_SDK_VERSION,
)


@dataclass(frozen=True, slots=True)
class Compatibility:
    engine_version: str
    ops_api_version: str
    openapi_digest: str
    contract_format: str
    sdk_version: str


compatibility = Compatibility(
    # Wired to the real engine version by the phase-6 compatibility matrix.
    engine_version="0.0.0-unset",
    ops_api_version=GENERATED_OPENAPI_VERSION,
    openapi_digest=GENERATED_OPENAPI_DIGEST,
    contract_format="templiqx/v1alpha1",
    sdk_version=GENERATED_SDK_VERSION,
)


def assert_compatibility() -> None:
    assert len(compatibility.openapi_digest) > len("sha256:"), "OpenAPI digest is empty"
    assert compatibility.openapi_digest == GENERATED_OPENAPI_DIGEST, (
        "Compatibility digest does not match the generated DTO marker"
    )


assert_compatibility()

