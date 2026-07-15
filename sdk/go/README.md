# Templiqx Go adapter

Thin, transport-only client for the Templiqx Operations HTTP API. Runtime code
uses only `net/http` and `encoding/json`; Templiqx semantics stay on the server.

```bash
go get github.com/blinqx/templiqx-adapter-go
```

```go
client, err := templiqx.NewClient("http://127.0.0.1:8080")
if err != nil { log.Fatal(err) }

inputs := map[string]templiqx.JsonValue{"name": "Ryan"}
fixture := templiqx.JsonValue(map[string]any{"greeting": "Hello Ryan"})
stream := false
response, err := client.ExecuteContract(context.Background(), "demo", "greeting",
    templiqx.ExecuteRequest{
        Render: &templiqx.RenderRequest{Inputs: &inputs},
        FixtureOutput: &fixture,
        Stream: &stream,
    },
    templiqx.WithRequestID("execute-example"),
)
if err != nil {
    var httpErr *templiqx.TempliqxHTTPError
    var transportErr *templiqx.TempliqxTransportError
    switch {
    case errors.As(err, &httpErr): log.Printf("status=%d diagnostics=%v", httpErr.StatusCode, httpErr.Envelope)
    case errors.As(err, &transportErr): log.Printf("request=%s cause=%v", transportErr.RequestID, transportErr.Unwrap())
    default: log.Print(err)
    }
    return
}
log.Printf("request=%s receipt=%+v", response.RequestID, response.Data.Result)
```

Every method takes `context.Context` first. `WithRequestID` overrides the random
UUID. Mandatory CAS methods require `WithIfMatch(fingerprint)`; `PutContract`
accepts it optionally. The client performs no retries or contract validation.

DTOs and compatibility markers are checked in and generated with pinned
`oapi-codegen` in types-only mode:

```bash
cd sdk/go
go generate ./...
./scripts/check-generated.sh
```
