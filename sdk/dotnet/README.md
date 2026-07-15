# Templiqx.Adapter

Thin asynchronous .NET transport client for the Templiqx Operations API. The
OpenAPI generator owns DTOs only; `TempliqxClient` is handwritten so request
ownership, timeout, cancellation, and error semantics remain explicit.

```csharp
using Templiqx.Adapter;
using Templiqx.Adapter.Generated;

using var templiqx = new TempliqxClient("http://localhost:8080");
var response = await templiqx.ExecuteContractAsync(
    "demo",
    "greeting",
    new ExecuteRequest
    {
        Capabilities = ["structured_output"],
    });
```

Regenerate and check the checked-in DTOs from the repository root:

```sh
sdk/dotnet/scripts/generate.sh
sdk/dotnet/scripts/generate.sh --check
```

NSwag is pinned in the SDK-local .NET tool manifest, but its DTO-only output for
this OpenAPI 3.1 document is not compilable: it references an undefined
`Stream_events` type and drops the `StreamEvent` one-of payloads. The script
therefore uses the permitted fallback, OpenAPI Generator 7.17.0 with
`--global-property models` (plus its generated `Option` and `ClientUtils` model
helpers). The generator artifact is checksum-pinned, and the generated
framework-neutral models request the newest framework label that Generator
7.17.0 recognizes (`net9.0`) but are compiled by this project for `net10.0`.
The script also records the raw-spec SHA-256 digest and the package version from
`Templiqx.Adapter.csproj`.
`Generated/OperationsV1.cs` is generated output and must not be hand-edited.

The client owns and disposes the `HttpClient` it creates. An injected
`HttpClient` remains caller-owned. Every operation accepts a cancellation token
and an optional per-call timeout; the default timeout is 30 seconds. Retry,
authentication, authorization, tenant policy, and provider credentials are host
policy and intentionally remain outside this SDK.
