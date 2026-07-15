# templiqx-adapter

Thin synchronous Python transport client for the Templiqx Operations API.

```sh
uv add templiqx-adapter
```

```python
from templiqx_adapter import (
    ExecuteRequest,
    JsonValue,
    RenderRequest,
    TempliqxClient,
    TempliqxHttpError,
    TempliqxTransportError,
)

with TempliqxClient("http://localhost:8080") as templiqx:
    try:
        response = templiqx.execute_contract(
            "demo",
            "greeting",
            ExecuteRequest(
                render=RenderRequest(inputs={"name": JsonValue(root="Ryan")}),
                capabilities=["structured_output"],
                fixture_output=JsonValue(root={"greeting": "Hello Ryan"}),
            ),
        )
        print(response.request_id, response.data.result.output_fingerprint)
    except TempliqxHttpError as error:
        print(error.envelope.diagnostics if error.envelope else error.raw_body)
    except TempliqxTransportError as error:
        print(error.request_id, error.__cause__)
```

Regenerate and check the checked-in DTOs from the repository root:

```sh
uv run --project sdk/python python sdk/python/scripts/generate.py
uv run --project sdk/python python sdk/python/scripts/generate.py --check
```

The client owns clients it creates; an injected `httpx.Client` remains caller-owned.
Per-call timeouts are supported. Hard cancellation of synchronous calls is caller-owned;
an async client is intentionally deferred. Templiqx retains contract semantics, CAS,
diagnostics, fingerprints, capability checks, authorization, and retry policy.

