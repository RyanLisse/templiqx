"""Transport and HTTP error mapping for the Templiqx Operations API."""

from __future__ import annotations

from ._generated.operations_v1 import OperationEnvelopeBase


class TempliqxTransportError(Exception):
    """A request failed before an HTTP response was received."""

    request_id: str

    def __init__(self, request_id: str) -> None:
        super().__init__("Templiqx request failed before receiving an HTTP response")
        self.request_id = request_id


class TempliqxHttpError(Exception):
    """A non-successful HTTP response from the Templiqx transport."""

    status_code: int
    envelope: OperationEnvelopeBase | None
    raw_body: str | None
    request_id: str

    def __init__(
        self,
        *,
        status_code: int,
        envelope: OperationEnvelopeBase | None,
        raw_body: str | None,
        request_id: str,
    ) -> None:
        super().__init__(f"Templiqx request failed with HTTP {status_code}")
        self.status_code = status_code
        self.envelope = envelope
        self.raw_body = raw_body
        self.request_id = request_id

