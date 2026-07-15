using System.Net;
using System.Text;
using System.Text.Json;
using Templiqx.Adapter.Generated;

namespace Templiqx.Adapter.Tests;

public sealed class TempliqxClientTests
{
    [Fact]
    public async Task ExecuteContractBuildsRequestAndDecodesEnvelope()
    {
        HttpRequestMessage? capturedRequest = null;
        string? capturedBody = null;
        var handler = new StubHttpMessageHandler(async (request, cancellationToken) =>
        {
            capturedRequest = request;
            capturedBody = await request.Content!.ReadAsStringAsync(cancellationToken);
            return new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent(
                    """
                    {
                      "api_version": "templiqx/v1alpha1",
                      "operation": "execute_contract",
                      "ok": true,
                      "diagnostics": [],
                      "fingerprints": {},
                      "result": {
                        "adapter": {
                          "id": "fixture",
                          "version": "1.0.0",
                          "capabilities": ["structured_output"]
                        },
                        "request_fingerprint": "sha256:request",
                        "output_fingerprint": "sha256:output",
                        "output": {},
                        "output_schema_valid": true
                      }
                    }
                    """,
                    Encoding.UTF8,
                    "application/json"),
            };
        });
        using var httpClient = new HttpClient(handler);
        using var client = new TempliqxClient("https://templiqx.example/", httpClient);

        var response = await client.ExecuteContractAsync(
            "demo package",
            "greeting",
            new ExecuteRequest
            {
                Capabilities = ["structured_output"],
            },
            requestId: "sdk-request-42",
            cancellationToken: CancellationToken.None);

        Assert.NotNull(capturedRequest);
        Assert.Equal(HttpMethod.Post, capturedRequest.Method);
        Assert.Equal(
            "https://templiqx.example/operations/v1/packages/demo%20package/contracts/greeting/execute",
            capturedRequest.RequestUri!.AbsoluteUri);
        Assert.Equal("sdk-request-42", capturedRequest.Headers.GetValues("X-Request-Id").Single());

        using var body = JsonDocument.Parse(capturedBody!);
        Assert.Equal(
            "structured_output",
            body.RootElement.GetProperty("capabilities")[0].GetString());
        Assert.False(body.RootElement.TryGetProperty("stream", out _));

        Assert.True(response.Ok);
        Assert.Equal("execute_contract", response.Operation);
        Assert.Equal("sha256:output", response.Result!.OutputFingerprint);
    }

    [Fact]
    public void GeneratedCompatibilityMarkerPassesSelfCheck()
    {
        Compat.AssertCompatibility();
        Assert.StartsWith("sha256:", Compat.Current.OpenApiDigest, StringComparison.Ordinal);
    }

    private sealed class StubHttpMessageHandler(
        Func<HttpRequestMessage, CancellationToken, Task<HttpResponseMessage>> sendAsync)
        : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => sendAsync(request, cancellationToken);
    }
}
