using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Templiqx.Adapter.Generated;

namespace Templiqx.Adapter;

/// <summary>A thin asynchronous transport over <see cref="HttpClient"/>.</summary>
public sealed class TempliqxClient : IDisposable
{
    private static readonly JsonSerializerOptions SerializerOptions = CreateSerializerOptions();

    private readonly string _baseUrl;
    private readonly HttpClient _httpClient;
    private readonly bool _ownsHttpClient;
    private readonly TimeSpan _defaultTimeout;
    private readonly IReadOnlyDictionary<string, string> _defaultHeaders;
    private bool _disposed;

    public TempliqxClient(
        string baseUrl,
        TimeSpan? timeout = null,
        IReadOnlyDictionary<string, string>? defaultHeaders = null)
        : this(
            baseUrl,
            new HttpClient { Timeout = Timeout.InfiniteTimeSpan },
            ownsHttpClient: true,
            timeout,
            defaultHeaders)
    {
    }

    public TempliqxClient(
        string baseUrl,
        HttpClient httpClient,
        TimeSpan? timeout = null,
        IReadOnlyDictionary<string, string>? defaultHeaders = null)
        : this(baseUrl, httpClient, ownsHttpClient: false, timeout, defaultHeaders)
    {
    }

    private TempliqxClient(
        string baseUrl,
        HttpClient httpClient,
        bool ownsHttpClient,
        TimeSpan? timeout,
        IReadOnlyDictionary<string, string>? defaultHeaders)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(baseUrl);
        ArgumentNullException.ThrowIfNull(httpClient);

        _baseUrl = baseUrl.TrimEnd('/');
        _httpClient = httpClient;
        _ownsHttpClient = ownsHttpClient;
        _defaultTimeout = timeout ?? TimeSpan.FromSeconds(30);
        if (_defaultTimeout <= TimeSpan.Zero && _defaultTimeout != Timeout.InfiniteTimeSpan)
        {
            throw new ArgumentOutOfRangeException(nameof(timeout), "Timeout must be positive or infinite.");
        }

        _defaultHeaders = defaultHeaders ?? new Dictionary<string, string>();
    }

    public Task<HealthStatus> LivenessAsync(
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<HealthStatus>(HttpMethod.Get, "/operations/v1/health/live", requestId, timeout, cancellationToken);

    public Task<HealthStatus> ReadinessAsync(
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<HealthStatus>(HttpMethod.Get, "/operations/v1/health/ready", requestId, timeout, cancellationToken);

    public Task<JsonElement> OpenApiAsync(
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonElement>(HttpMethod.Get, "/operations/v1/openapi.json", requestId, timeout, cancellationToken);

    public Task<CatalogEnvelope> CatalogAsync(
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<CatalogEnvelope>(HttpMethod.Get, "/operations/v1/catalog", requestId, timeout, cancellationToken);

    public Task<PackageListEnvelope> DiscoverPackagesAsync(
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<PackageListEnvelope>(HttpMethod.Get, "/operations/v1/packages", requestId, timeout, cancellationToken);

    public Task<PackageEnvelope> CreatePackageAsync(
        CreatePackageRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<PackageEnvelope>(HttpMethod.Post, "/operations/v1/packages", requestId, timeout, cancellationToken, body);

    public Task<ContractEnvelope> InspectContractAsync(
        string package,
        string contract,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<ContractEnvelope>(HttpMethod.Get, ContractPath(package, contract), requestId, timeout, cancellationToken);

    public Task<SummaryEnvelope> PutContractAsync(
        string package,
        string contract,
        string source,
        string? ifMatch = null,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<SummaryEnvelope>(
            HttpMethod.Put,
            ContractPath(package, contract),
            requestId,
            timeout,
            cancellationToken,
            source,
            "application/yaml",
            ifMatch);

    public Task<SummaryEnvelope> DeleteContractAsync(
        string package,
        string contract,
        string ifMatch,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<SummaryEnvelope>(
            HttpMethod.Delete,
            ContractPath(package, contract),
            requestId,
            timeout,
            cancellationToken,
            ifMatch: ifMatch);

    public Task<SummaryEnvelope> ValidateContractAsync(
        string package,
        string contract,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<SummaryEnvelope>(HttpMethod.Post, $"{ContractPath(package, contract)}/validate", requestId, timeout, cancellationToken);

    public Task<CompiledInteractionEnvelope> CompileContractAsync(
        string package,
        string contract,
        CompileRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<CompiledInteractionEnvelope>(HttpMethod.Post, $"{ContractPath(package, contract)}/compile", requestId, timeout, cancellationToken, body);

    public Task<ExecutionReceiptEnvelope> ExecuteContractAsync(
        string package,
        string contract,
        ExecuteRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<ExecutionReceiptEnvelope>(HttpMethod.Post, $"{ContractPath(package, contract)}/execute", requestId, timeout, cancellationToken, body);

    public Task<PackageEnvelope> UpdatePackageAsync(
        string package,
        UpdatePackageRequest body,
        string ifMatch,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<PackageEnvelope>(HttpMethod.Patch, PackagePath(package), requestId, timeout, cancellationToken, body, ifMatch: ifMatch);

    public Task<PackageEnvelope> DeletePackageAsync(
        string package,
        string ifMatch,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<PackageEnvelope>(HttpMethod.Delete, PackagePath(package), requestId, timeout, cancellationToken, ifMatch: ifMatch);

    public Task<JsonValueEnvelope> ValidatePackageAsync(
        string package,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, $"{PackagePath(package)}/validate", requestId, timeout, cancellationToken);

    public Task<JsonValueEnvelope> TestPackageAsync(
        string package,
        CapabilitiesRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, $"{PackagePath(package)}/test", requestId, timeout, cancellationToken, body);

    public Task<JsonValueEnvelope> ExportPackageIdentityAsync(
        string package,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Get, $"{PackagePath(package)}/identity", requestId, timeout, cancellationToken);

    public Task<JsonValueEnvelope> SignPackageAsync(
        string package,
        SignPackageRequest body,
        string ifMatch,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, $"{PackagePath(package)}/sign", requestId, timeout, cancellationToken, body, ifMatch: ifMatch);

    public Task<JsonValueEnvelope> VerifyPackageTrustAsync(
        string package,
        VerifyPackageTrustRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, $"{PackagePath(package)}/verify-trust", requestId, timeout, cancellationToken, body);

    public Task<JsonValueEnvelope> ListEvalsAsync(
        string package,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Get, $"{PackagePath(package)}/evals", requestId, timeout, cancellationToken);

    public Task<JsonValueEnvelope> RunEvalAsync(
        string package,
        RunEvalRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, $"{PackagePath(package)}/evals/run", requestId, timeout, cancellationToken, body);

    public Task<QualityProposalReportEnvelope> AssessQualityProposalsAsync(
        string package,
        QualityProposalRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<QualityProposalReportEnvelope>(
            HttpMethod.Post,
            $"{PackagePath(package)}/quality/proposals:assess",
            requestId,
            timeout,
            cancellationToken,
            body);

    public Task<JsonValueEnvelope> RenderContractAsync(
        string package,
        string contract,
        CompileRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, $"{ContractPath(package, contract)}/render", requestId, timeout, cancellationToken, body);

    public Task<JsonValueEnvelope> DiffContractAsync(
        string package,
        string contract,
        DiffContractRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, $"{ContractPath(package, contract)}/diff", requestId, timeout, cancellationToken, body);

    public Task<JsonValueEnvelope> ExplainContractAsync(
        string package,
        string contract,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Get, $"{ContractPath(package, contract)}/explain", requestId, timeout, cancellationToken);

    public Task<JsonValueEnvelope> MigrateLegacyAsync(
        MigrateLegacyRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, "/operations/v1/legacy/migrate", requestId, timeout, cancellationToken, body);

    public Task<JsonValueEnvelope> RenderDocumentAsync(
        RenderDocumentRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(HttpMethod.Post, "/operations/v1/documents/render", requestId, timeout, cancellationToken, body);

    public Task<InspectDocumentEnvelope> InspectDocumentAsync(
        InspectDocumentRequest body,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<InspectDocumentEnvelope>(HttpMethod.Post, "/operations/v1/documents/inspect", requestId, timeout, cancellationToken, body);

    public Task<JsonValueEnvelope> ListWorkspaceArtifactsAsync(
        string package,
        string? workspace = null,
        string? prefix = null,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(
            HttpMethod.Get,
            WithQuery("/operations/v1/artifacts", ("package", package), ("workspace", workspace), ("prefix", prefix)),
            requestId,
            timeout,
            cancellationToken);

    public Task<JsonValueEnvelope> ReadArtifactAsync(
        string artifact,
        string package,
        string? workspace = null,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(
            HttpMethod.Get,
            WithQuery($"/operations/v1/artifacts/{ArtifactPath(artifact)}", ("package", package), ("workspace", workspace)),
            requestId,
            timeout,
            cancellationToken);

    public Task<JsonValueEnvelope> DeleteWorkspaceArtifactAsync(
        string artifact,
        string package,
        string ifMatch,
        string? workspace = null,
        string? requestId = null,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<JsonValueEnvelope>(
            HttpMethod.Delete,
            WithQuery($"/operations/v1/artifacts/{ArtifactPath(artifact)}", ("package", package), ("workspace", workspace)),
            requestId,
            timeout,
            cancellationToken,
            ifMatch: ifMatch);

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        if (_ownsHttpClient)
        {
            _httpClient.Dispose();
        }

        _disposed = true;
    }

    private async Task<T> SendAsync<T>(
        HttpMethod method,
        string path,
        string? requestId,
        TimeSpan? timeout,
        CancellationToken cancellationToken,
        object? body = null,
        string contentType = "application/json",
        string? ifMatch = null)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var outgoingRequestId = requestId ?? Guid.NewGuid().ToString();
        using var request = new HttpRequestMessage(method, $"{_baseUrl}{path}");

        foreach (var (name, value) in _defaultHeaders)
        {
            request.Headers.TryAddWithoutValidation(name, value);
        }

        request.Headers.Accept.Clear();
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/yaml"));
        request.Headers.Remove("X-Request-Id");
        request.Headers.TryAddWithoutValidation("X-Request-Id", outgoingRequestId);

        if (ifMatch is not null)
        {
            request.Headers.TryAddWithoutValidation("If-Match", ifMatch);
        }

        if (body is string text && contentType != "application/json")
        {
            request.Content = new StringContent(text, Encoding.UTF8, contentType);
        }
        else if (body is not null)
        {
            var json = JsonSerializer.Serialize(body, body.GetType(), SerializerOptions);
            request.Content = new StringContent(json, Encoding.UTF8, contentType);
        }

        var effectiveTimeout = timeout ?? _defaultTimeout;
        if (effectiveTimeout <= TimeSpan.Zero && effectiveTimeout != Timeout.InfiniteTimeSpan)
        {
            throw new ArgumentOutOfRangeException(nameof(timeout), "Timeout must be positive or infinite.");
        }

        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        if (effectiveTimeout != Timeout.InfiniteTimeSpan)
        {
            timeoutSource.CancelAfter(effectiveTimeout);
        }

        HttpResponseMessage response;
        try
        {
            response = await _httpClient.SendAsync(
                request,
                HttpCompletionOption.ResponseHeadersRead,
                timeoutSource.Token).ConfigureAwait(false);
        }
        catch (Exception error) when (error is HttpRequestException or OperationCanceledException)
        {
            throw new TempliqxTransportError(outgoingRequestId, error);
        }

        using (response)
        {
            var effectiveRequestId = ResponseRequestId(response) ?? outgoingRequestId;
            string rawBody;
            try
            {
                rawBody = await response.Content.ReadAsStringAsync(timeoutSource.Token).ConfigureAwait(false);
            }
            catch (Exception error) when (error is HttpRequestException or OperationCanceledException)
            {
                throw new TempliqxTransportError(effectiveRequestId, error);
            }

            if (!response.IsSuccessStatusCode)
            {
                var envelope = TryReadEnvelope(rawBody);
                throw new TempliqxHttpError(
                    (int)response.StatusCode,
                    envelope,
                    envelope is null ? rawBody : null,
                    effectiveRequestId);
            }

            var value = JsonSerializer.Deserialize<T>(rawBody, SerializerOptions);
            return value ?? throw new JsonException("Templiqx returned an empty response body.");
        }
    }

    private static OperationEnvelopeBase? TryReadEnvelope(string rawBody)
    {
        try
        {
            using var document = JsonDocument.Parse(rawBody);
            if (document.RootElement.ValueKind != JsonValueKind.Object ||
                !document.RootElement.TryGetProperty("diagnostics", out var diagnostics) ||
                diagnostics.ValueKind != JsonValueKind.Array)
            {
                return null;
            }

            return JsonSerializer.Deserialize<OperationEnvelopeBase>(rawBody, SerializerOptions);
        }
        catch (Exception error) when (error is JsonException or ArgumentException)
        {
            return null;
        }
    }

    private static JsonSerializerOptions CreateSerializerOptions()
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web)
        {
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        };

        var generatedConverters = typeof(OperationEnvelopeBase).Assembly
            .GetTypes()
            .Where(type =>
                type.Namespace == typeof(OperationEnvelopeBase).Namespace &&
                !type.IsAbstract &&
                typeof(JsonConverter).IsAssignableFrom(type) &&
                type.GetConstructor(Type.EmptyTypes) is not null)
            .OrderBy(type => type.FullName, StringComparer.Ordinal);

        foreach (var converterType in generatedConverters)
        {
            options.Converters.Add((JsonConverter)Activator.CreateInstance(converterType)!);
        }

        return options;
    }

    private static string? ResponseRequestId(HttpResponseMessage response) =>
        response.Headers.TryGetValues("X-Request-Id", out var values) ? values.FirstOrDefault() : null;

    private static string PackagePath(string package) => $"/operations/v1/packages/{Segment(package)}";

    private static string ContractPath(string package, string contract) =>
        $"{PackagePath(package)}/contracts/{Segment(contract)}";

    private static string Segment(string value) => Uri.EscapeDataString(value);

    private static string ArtifactPath(string value) => string.Join('/', value.Split('/').Select(Segment));

    private static string WithQuery(string path, params (string Name, string? Value)[] values)
    {
        var query = string.Join(
            '&',
            values
                .Where(pair => pair.Value is not null)
                .Select(pair => $"{Segment(pair.Name)}={Segment(pair.Value!)}"));
        return query.Length == 0 ? path : $"{path}?{query}";
    }
}
