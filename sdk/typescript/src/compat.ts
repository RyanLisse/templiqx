import {
  GENERATED_CONTRACT_FORMAT,
  GENERATED_ENGINE_API_VERSION,
  GENERATED_ENGINE_VERSION,
  GENERATED_OPENAPI_DIGEST,
  GENERATED_OPENAPI_VERSION,
  GENERATED_SDK_VERSION,
} from "./generated/operations-v1.js";

export const compatibility = Object.freeze({
  engineApiVersion: GENERATED_ENGINE_API_VERSION,
  engineVersion: GENERATED_ENGINE_VERSION,
  opsApiVersion: GENERATED_OPENAPI_VERSION,
  openApiDigest: GENERATED_OPENAPI_DIGEST,
  contractFormat: GENERATED_CONTRACT_FORMAT,
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
  assert(compatibility.engineVersion === GENERATED_ENGINE_VERSION, "Compatibility engine version does not match the generated marker");
}

assertCompatibility();
