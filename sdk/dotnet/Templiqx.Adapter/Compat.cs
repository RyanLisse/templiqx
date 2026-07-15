using Templiqx.Adapter.Generated;

namespace Templiqx.Adapter;

public sealed record Compatibility(
    string EngineApiVersion,
    string EngineVersion,
    string OpsApiVersion,
    string OpenApiDigest,
    string ContractFormat,
    string SdkVersion);

public static class Compat
{
    public static Compatibility Current { get; } = new(
        EngineApiVersion: GeneratedMeta.GeneratedEngineApiVersion,
        EngineVersion: GeneratedMeta.GeneratedEngineVersion,
        OpsApiVersion: GeneratedMeta.GeneratedOpenApiVersion,
        OpenApiDigest: GeneratedMeta.GeneratedOpenApiDigest,
        ContractFormat: GeneratedMeta.GeneratedContractFormat,
        SdkVersion: GeneratedMeta.GeneratedSdkVersion);

    public static void AssertCompatibility()
    {
        if (Current.OpenApiDigest.Length <= "sha256:".Length)
        {
            throw new InvalidOperationException("OpenAPI digest is empty");
        }

        if (!string.Equals(
                Current.OpenApiDigest,
                GeneratedMeta.GeneratedOpenApiDigest,
                StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Compatibility digest does not match the generated DTO marker");
        }

        if (!string.Equals(
                Current.EngineVersion,
                GeneratedMeta.GeneratedEngineVersion,
                StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Compatibility engine version does not match the generated marker");
        }
    }
}
