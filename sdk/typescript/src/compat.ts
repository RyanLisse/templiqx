import {
  GENERATED_OPENAPI_DIGEST,
  GENERATED_OPENAPI_VERSION,
  GENERATED_SDK_VERSION,
} from "./generated/operations-v1.js";

export const compatibility = Object.freeze({
  // TODO(phase-6): replace with the supported engine-version range.
  engineVersion: "TODO-phase-6",
  opsApiVersion: GENERATED_OPENAPI_VERSION,
  openApiDigest: GENERATED_OPENAPI_DIGEST,
  contractFormat: "templiqx/v1alpha1",
  sdkVersion: GENERATED_SDK_VERSION,
} as const);

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

export function assertCompatibility(): void {
  assert(compatibility.openApiDigest.length > "sha256:".length, "OpenAPI digest is empty");
  assert(
    compatibility.openApiDigest === GENERATED_OPENAPI_DIGEST,
    "Compatibility digest does not match the generated DTO marker",
  );
}

assertCompatibility();
