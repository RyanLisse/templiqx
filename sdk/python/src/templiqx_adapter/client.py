"""Synchronous transport façade for all Templiqx Operations API methods."""

from __future__ import annotations

import uuid
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from types import TracebackType
from typing import Any, Generic, TypeVar, cast
from urllib.parse import quote, urlencode

import httpx
from pydantic import BaseModel, ValidationError

from ._generated.operations_v1 import (
    CapabilitiesRequest,
    CatalogEnvelope,
    CompileRequest,
    CompiledInteractionEnvelope,
    ContractEnvelope,
    CreatePackageRequest,
    DiffContractRequest,
    ExecuteRequest,
    ExecutionReceiptEnvelope,
    HealthStatus,
    InspectDocumentEnvelope,
    InspectDocumentRequest,
    JsonValueEnvelope,
    MigrateLegacyRequest,
    OperationEnvelopeBase,
    PackageEnvelope,
    PackageListEnvelope,
    QualityProposalReportEnvelope,
    QualityProposalRequest,
    RenderDocumentRequest,
    RunEvalRequest,
    SignPackageRequest,
    SummaryEnvelope,
    UpdatePackageRequest,
    VerifyPackageTrustRequest,
)
from .errors import TempliqxHttpError, TempliqxTransportError


ResponseT = TypeVar("ResponseT")
ModelT = TypeVar("ModelT", bound=BaseModel)
Decoder = Callable[[httpx.Response], ResponseT]


@dataclass(frozen=True, slots=True)
class TempliqxResponse(Generic[ResponseT]):
    data: ResponseT
    request_id: str


def _model_decoder(model: type[ModelT]) -> Decoder[ModelT]:
    def decode(response: httpx.Response) -> ModelT:
        return model.model_validate(response.json())

    return decode


def _text_decoder(response: httpx.Response) -> str:
    return response.text


def _object_decoder(response: httpx.Response) -> dict[str, Any]:
    return cast(dict[str, Any], response.json())


def _segment(value: str) -> str:
    return quote(value, safe="")


def _artifact_path(value: str) -> str:
    return "/".join(_segment(part) for part in value.split("/"))


class TempliqxClient:
    """A thin synchronous client over ``httpx.Client``."""

    def __init__(
        self,
        base_url: str,
        *,
        client: httpx.Client | None = None,
        timeout: float = 30.0,
        default_headers: Mapping[str, str] | None = None,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._client = client if client is not None else httpx.Client()
        self._owns_client = client is None
        self._timeout = timeout
        self._default_headers = httpx.Headers(default_headers)

    def __enter__(self) -> TempliqxClient:
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        self.close()

    def close(self) -> None:
        if self._owns_client:
            self._client.close()

    def _dispatch(
        self,
        method: str,
        path: str,
        decoder: Decoder[ResponseT],
        *,
        body: BaseModel | str | None = None,
        content_type: str = "application/json",
        timeout: float | None = None,
        request_id: str | None = None,
        if_match: str | None = None,
    ) -> TempliqxResponse[ResponseT]:
        outgoing_request_id = request_id or str(uuid.uuid4())
        headers = httpx.Headers(self._default_headers)
        headers["accept"] = "application/json, application/yaml"
        headers["x-request-id"] = outgoing_request_id
        if if_match is not None:
            headers["if-match"] = if_match

        content: str | None = None
        if isinstance(body, BaseModel):
            headers["content-type"] = content_type
            content = body.model_dump_json(by_alias=True, exclude_unset=True)
        elif body is not None:
            headers["content-type"] = content_type
            content = body

        try:
            response = self._client.request(
                method,
                f"{self._base_url}{path}",
                headers=headers,
                content=content,
                timeout=self._timeout if timeout is None else timeout,
            )
        except httpx.TransportError as error:
            raise TempliqxTransportError(outgoing_request_id) from error

        effective_request_id = response.headers.get("x-request-id", outgoing_request_id)
        if not response.is_success:
            raw_body = response.text
            envelope: OperationEnvelopeBase | None = None
            try:
                envelope = OperationEnvelopeBase.model_validate(response.json())
            except (ValueError, ValidationError):
                pass
            raise TempliqxHttpError(
                status_code=response.status_code,
                envelope=envelope,
                raw_body=None if envelope is not None else raw_body,
                request_id=effective_request_id,
            )

        return TempliqxResponse(data=decoder(response), request_id=effective_request_id)

    @staticmethod
    def _package_path(package: str) -> str:
        return f"/operations/v1/packages/{_segment(package)}"

    @classmethod
    def _contract_path(cls, package: str, contract: str) -> str:
        return f"{cls._package_path(package)}/contracts/{_segment(contract)}"

    @staticmethod
    def _query(path: str, values: Mapping[str, str | None]) -> str:
        query = urlencode({key: value for key, value in values.items() if value is not None})
        return f"{path}?{query}" if query else path

    def get_operations_v1_liveness(
        self, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[HealthStatus]:
        return self._dispatch(
            "GET", "/operations/v1/health/live", _model_decoder(HealthStatus), timeout=timeout, request_id=request_id
        )

    def get_operations_v1_readiness(
        self, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[HealthStatus]:
        return self._dispatch(
            "GET", "/operations/v1/health/ready", _model_decoder(HealthStatus), timeout=timeout, request_id=request_id
        )

    def get_operations_v1_open_api_yaml(
        self, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[str]:
        return self._dispatch(
            "GET", "/operations/v1/openapi.yaml", _text_decoder, timeout=timeout, request_id=request_id
        )

    def get_operations_v1_open_api(
        self, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[dict[str, Any]]:
        return self._dispatch(
            "GET", "/operations/v1/openapi.json", _object_decoder, timeout=timeout, request_id=request_id
        )

    def catalog(
        self, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[CatalogEnvelope]:
        return self._dispatch(
            "GET", "/operations/v1/catalog", _model_decoder(CatalogEnvelope), timeout=timeout, request_id=request_id
        )

    def discover_packages(
        self, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[PackageListEnvelope]:
        return self._dispatch(
            "GET", "/operations/v1/packages", _model_decoder(PackageListEnvelope), timeout=timeout, request_id=request_id
        )

    def create_package(
        self,
        body: CreatePackageRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[PackageEnvelope]:
        return self._dispatch(
            "POST",
            "/operations/v1/packages",
            _model_decoder(PackageEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def inspect_contract(
        self,
        package: str,
        contract: str,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[ContractEnvelope]:
        return self._dispatch(
            "GET",
            self._contract_path(package, contract),
            _model_decoder(ContractEnvelope),
            timeout=timeout,
            request_id=request_id,
        )

    def put_contract(
        self,
        package: str,
        contract: str,
        source: str,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
        if_match: str | None = None,
    ) -> TempliqxResponse[SummaryEnvelope]:
        return self._dispatch(
            "PUT",
            self._contract_path(package, contract),
            _model_decoder(SummaryEnvelope),
            body=source,
            content_type="application/yaml",
            timeout=timeout,
            request_id=request_id,
            if_match=if_match,
        )

    def delete_contract(
        self,
        package: str,
        contract: str,
        *,
        if_match: str,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[SummaryEnvelope]:
        return self._dispatch(
            "DELETE",
            self._contract_path(package, contract),
            _model_decoder(SummaryEnvelope),
            timeout=timeout,
            request_id=request_id,
            if_match=if_match,
        )

    def validate_contract(
        self,
        package: str,
        contract: str,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[SummaryEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._contract_path(package, contract)}/validate",
            _model_decoder(SummaryEnvelope),
            timeout=timeout,
            request_id=request_id,
        )

    def compile_contract(
        self,
        package: str,
        contract: str,
        body: CompileRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[CompiledInteractionEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._contract_path(package, contract)}/compile",
            _model_decoder(CompiledInteractionEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def execute_contract(
        self,
        package: str,
        contract: str,
        body: ExecuteRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[ExecutionReceiptEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._contract_path(package, contract)}/execute",
            _model_decoder(ExecutionReceiptEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def update_package(
        self,
        package: str,
        body: UpdatePackageRequest,
        *,
        if_match: str,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[PackageEnvelope]:
        return self._dispatch(
            "PATCH",
            self._package_path(package),
            _model_decoder(PackageEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
            if_match=if_match,
        )

    def delete_package(
        self,
        package: str,
        *,
        if_match: str,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[PackageEnvelope]:
        return self._dispatch(
            "DELETE",
            self._package_path(package),
            _model_decoder(PackageEnvelope),
            timeout=timeout,
            request_id=request_id,
            if_match=if_match,
        )

    def validate_package(
        self, package: str, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._package_path(package)}/validate",
            _model_decoder(JsonValueEnvelope),
            timeout=timeout,
            request_id=request_id,
        )

    def test_package(
        self,
        package: str,
        body: CapabilitiesRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._package_path(package)}/test",
            _model_decoder(JsonValueEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def export_package_identity(
        self, package: str, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "GET",
            f"{self._package_path(package)}/identity",
            _model_decoder(JsonValueEnvelope),
            timeout=timeout,
            request_id=request_id,
        )

    def sign_package(
        self,
        package: str,
        body: SignPackageRequest,
        *,
        if_match: str,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._package_path(package)}/sign",
            _model_decoder(JsonValueEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
            if_match=if_match,
        )

    def verify_package_trust(
        self,
        package: str,
        body: VerifyPackageTrustRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._package_path(package)}/verify-trust",
            _model_decoder(JsonValueEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def list_evals(
        self, package: str, *, timeout: float | None = None, request_id: str | None = None
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "GET",
            f"{self._package_path(package)}/evals",
            _model_decoder(JsonValueEnvelope),
            timeout=timeout,
            request_id=request_id,
        )

    def run_eval(
        self,
        package: str,
        body: RunEvalRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._package_path(package)}/evals/run",
            _model_decoder(JsonValueEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def assess_quality_proposals(
        self,
        package: str,
        body: QualityProposalRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[QualityProposalReportEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._package_path(package)}/quality/proposals:assess",
            _model_decoder(QualityProposalReportEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def render_contract(
        self,
        package: str,
        contract: str,
        body: CompileRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._contract_path(package, contract)}/render",
            _model_decoder(JsonValueEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def diff_contract(
        self,
        package: str,
        contract: str,
        body: DiffContractRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            f"{self._contract_path(package, contract)}/diff",
            _model_decoder(JsonValueEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def explain_contract(
        self,
        package: str,
        contract: str,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "GET",
            f"{self._contract_path(package, contract)}/explain",
            _model_decoder(JsonValueEnvelope),
            timeout=timeout,
            request_id=request_id,
        )

    def migrate_legacy(
        self,
        body: MigrateLegacyRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            "/operations/v1/legacy/migrate",
            _model_decoder(JsonValueEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def render_document(
        self,
        body: RenderDocumentRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        return self._dispatch(
            "POST",
            "/operations/v1/documents/render",
            _model_decoder(JsonValueEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def inspect_document(
        self,
        body: InspectDocumentRequest,
        *,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[InspectDocumentEnvelope]:
        return self._dispatch(
            "POST",
            "/operations/v1/documents/inspect",
            _model_decoder(InspectDocumentEnvelope),
            body=body,
            timeout=timeout,
            request_id=request_id,
        )

    def list_workspace_artifacts(
        self,
        package: str,
        *,
        workspace: str | None = None,
        prefix: str | None = None,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        path = self._query(
            "/operations/v1/artifacts",
            {"package": package, "workspace": workspace, "prefix": prefix},
        )
        return self._dispatch(
            "GET", path, _model_decoder(JsonValueEnvelope), timeout=timeout, request_id=request_id
        )

    def read_artifact(
        self,
        artifact: str,
        *,
        package: str,
        workspace: str | None = None,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        path = self._query(
            f"/operations/v1/artifacts/{_artifact_path(artifact)}",
            {"package": package, "workspace": workspace},
        )
        return self._dispatch(
            "GET", path, _model_decoder(JsonValueEnvelope), timeout=timeout, request_id=request_id
        )

    def delete_workspace_artifact(
        self,
        artifact: str,
        *,
        package: str,
        if_match: str,
        workspace: str | None = None,
        timeout: float | None = None,
        request_id: str | None = None,
    ) -> TempliqxResponse[JsonValueEnvelope]:
        path = self._query(
            f"/operations/v1/artifacts/{_artifact_path(artifact)}",
            {"package": package, "workspace": workspace},
        )
        return self._dispatch(
            "DELETE",
            path,
            _model_decoder(JsonValueEnvelope),
            timeout=timeout,
            request_id=request_id,
            if_match=if_match,
        )
