using Templiqx.Adapter.Generated;

namespace Templiqx.Adapter;

public sealed record Compatibility(
    string EngineVersion,
    string OpsApiVersion,
    string OpenApiDigest,
    string ContractFormat,
    string SdkVersion);

public static class Compat
{
    public static Compatibility Current { get; } = new(
        EngineVersion: "TODO-phase-6",
        OpsApiVersion: GeneratedMeta.GeneratedOpenApiVersion,
        OpenApiDigest: GeneratedMeta.GeneratedOpenApiDigest,
        ContractFormat: "templiqx/v1alpha1",
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
    }
}
