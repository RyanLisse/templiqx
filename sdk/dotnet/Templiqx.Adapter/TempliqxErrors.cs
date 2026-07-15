using Templiqx.Adapter.Generated;

namespace Templiqx.Adapter;

/// <summary>A request failed before an HTTP response was received.</summary>
public sealed class TempliqxTransportError : Exception
{
    public TempliqxTransportError(string requestId, Exception innerException)
        : base("Templiqx request failed before receiving an HTTP response", innerException)
    {
        RequestId = requestId;
    }

    public string RequestId { get; }
}

/// <summary>A non-successful HTTP response from the Templiqx transport.</summary>
public sealed class TempliqxHttpError : Exception
{
    public TempliqxHttpError(
        int statusCode,
        OperationEnvelopeBase? envelope,
        string? rawBody,
        string requestId)
        : base($"Templiqx request failed with HTTP {statusCode}")
    {
        StatusCode = statusCode;
        Envelope = envelope;
        RawBody = rawBody;
        RequestId = requestId;
    }

    public int StatusCode { get; }

    public OperationEnvelopeBase? Envelope { get; }

    public string? RawBody { get; }

    public string RequestId { get; }
}
