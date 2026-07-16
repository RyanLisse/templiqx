#!/usr/bin/env node
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import YAML from "yaml";
import { writeOpenApiReport } from "./report.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const parseYaml = (relativePath) => YAML.parse(read(relativePath), { prettyErrors: true });
const matrixPath = "openapi/compatibility-matrix.yaml";
const specPath = "openapi/templiqx-operations-v1.yaml";
const matrix = parseYaml(matrixPath);
const specText = read(specPath);
const spec = YAML.parse(specText, { prettyErrors: true });
const errors = [];
const fail = (message) => errors.push(message);

function expectEqual(label, actual, expected) {
  if (actual !== expected) fail(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}

function requireString(value, label) {
  if (typeof value !== "string" || value.length === 0) fail(`${label} must be a non-empty string`);
}

for (const field of [
  "opsApiVersion",
  "openApiDigest",
  "contractFormat",
  "engineApiVersion",
  "engineVersion",
]) {
  requireString(matrix?.[field], `matrix.${field}`);
}

const actualDigest = `sha256:${createHash("sha256").update(specText).digest("hex")}`;
expectEqual("matrix.openApiDigest", matrix?.openApiDigest, actualDigest);
expectEqual("matrix.opsApiVersion", matrix?.opsApiVersion, spec?.info?.version);
expectEqual(
  "matrix.contractFormat",
  matrix?.contractFormat,
  spec?.components?.schemas?.OperationEnvelopeBase?.properties?.api_version?.const,
);

const cargoVersion = read("Cargo.toml").match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
expectEqual("matrix.engineVersion", matrix?.engineVersion, cargoVersion);

const sdkDefinitions = {
  typescript: {
    manifestPath: "sdk/typescript/package.json",
    metadataPath: "sdk/typescript/src/generated/operations-v1.ts",
    compatibilityPath: "sdk/typescript/src/compat.ts",
    manifest(text) {
      const manifest = JSON.parse(text);
      return { package: manifest.name, sdkVersion: manifest.version };
    },
    markers: {
      opsApiVersion: /GENERATED_OPENAPI_VERSION\s*=\s*"([^"]+)"/,
      openApiDigest: /GENERATED_OPENAPI_DIGEST\s*=\s*"([^"]+)"/,
      contractFormat: /GENERATED_CONTRACT_FORMAT\s*=\s*"([^"]+)"/,
      engineApiVersion: /GENERATED_ENGINE_API_VERSION\s*=\s*"([^"]+)"/,
      engineVersion: /GENERATED_ENGINE_VERSION\s*=\s*"([^"]+)"/,
      sdkVersion: /GENERATED_SDK_VERSION\s*=\s*"([^"]+)"/,
    },
    wiring: {
      opsApiVersion: /opsApiVersion:\s*GENERATED_OPENAPI_VERSION/,
      openApiDigest: /openApiDigest:\s*GENERATED_OPENAPI_DIGEST/,
      contractFormat: /contractFormat:\s*GENERATED_CONTRACT_FORMAT/,
      engineApiVersion: /engineApiVersion:\s*GENERATED_ENGINE_API_VERSION/,
      engineVersion: /engineVersion:\s*GENERATED_ENGINE_VERSION/,
      sdkVersion: /sdkVersion:\s*GENERATED_SDK_VERSION/,
    },
  },
  dotnet: {
    manifestPath: "sdk/dotnet/Templiqx.Adapter/Templiqx.Adapter.csproj",
    metadataPath: "sdk/dotnet/Templiqx.Adapter/Generated/GeneratedMeta.cs",
    compatibilityPath: "sdk/dotnet/Templiqx.Adapter/Compat.cs",
    manifest(text) {
      return {
        package: text.match(/<PackageId>([^<]+)<\/PackageId>/)?.[1],
        sdkVersion: text.match(/<Version>([^<]+)<\/Version>/)?.[1],
      };
    },
    markers: {
      opsApiVersion: /GeneratedOpenApiVersion\s*=\s*"([^"]+)"/,
      openApiDigest: /GeneratedOpenApiDigest\s*=\s*"([^"]+)"/,
      contractFormat: /GeneratedContractFormat\s*=\s*"([^"]+)"/,
      engineApiVersion: /GeneratedEngineApiVersion\s*=\s*"([^"]+)"/,
      engineVersion: /GeneratedEngineVersion\s*=\s*"([^"]+)"/,
      sdkVersion: /GeneratedSdkVersion\s*=\s*"([^"]+)"/,
    },
    wiring: {
      opsApiVersion: /OpsApiVersion:\s*GeneratedMeta\.GeneratedOpenApiVersion/,
      openApiDigest: /OpenApiDigest:\s*GeneratedMeta\.GeneratedOpenApiDigest/,
      contractFormat: /ContractFormat:\s*GeneratedMeta\.GeneratedContractFormat/,
      engineApiVersion: /EngineApiVersion:\s*GeneratedMeta\.GeneratedEngineApiVersion/,
      engineVersion: /EngineVersion:\s*GeneratedMeta\.GeneratedEngineVersion/,
      sdkVersion: /SdkVersion:\s*GeneratedMeta\.GeneratedSdkVersion/,
    },
  },
  python: {
    manifestPath: "sdk/python/pyproject.toml",
    metadataPath: "sdk/python/src/templiqx_adapter/_generated/operations_v1.py",
    compatibilityPath: "sdk/python/src/templiqx_adapter/compat.py",
    manifest(text) {
      const project = text.match(/\[project\]([\s\S]*?)(?=\n\[|$)/)?.[1] ?? "";
      return {
        package: project.match(/^name\s*=\s*"([^"]+)"/m)?.[1],
        sdkVersion: project.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
      };
    },
    markers: {
      opsApiVersion: /GENERATED_OPENAPI_VERSION\s*=\s*['"]([^'"]+)['"]/,
      openApiDigest: /GENERATED_OPENAPI_DIGEST\s*=\s*['"]([^'"]+)['"]/,
      contractFormat: /GENERATED_CONTRACT_FORMAT\s*=\s*['"]([^'"]+)['"]/,
      engineApiVersion: /GENERATED_ENGINE_API_VERSION\s*=\s*['"]([^'"]+)['"]/,
      engineVersion: /GENERATED_ENGINE_VERSION\s*=\s*['"]([^'"]+)['"]/,
      sdkVersion: /GENERATED_SDK_VERSION\s*=\s*['"]([^'"]+)['"]/,
    },
    wiring: {
      opsApiVersion: /ops_api_version=GENERATED_OPENAPI_VERSION/,
      openApiDigest: /openapi_digest=GENERATED_OPENAPI_DIGEST/,
      contractFormat: /contract_format=GENERATED_CONTRACT_FORMAT/,
      engineApiVersion: /engine_api_version=GENERATED_ENGINE_API_VERSION/,
      engineVersion: /engine_version=GENERATED_ENGINE_VERSION/,
      sdkVersion: /sdk_version=GENERATED_SDK_VERSION/,
    },
  },
  go: {
    manifestPath: "sdk/go/go.mod",
    metadataPath: "sdk/go/compat_generated.go",
    compatibilityPath: "sdk/go/compat.go",
    manifest(text) {
      return {
        package: text.match(/^module\s+(\S+)/m)?.[1],
        sdkVersion: text.match(/templiqx-sdk-version:\s*(\S+)/)?.[1],
      };
    },
    markers: {
      opsApiVersion: /GeneratedOpenAPIVersion\s*=\s*"([^"]+)"/,
      openApiDigest: /GeneratedOpenAPIDigest\s*=\s*"([^"]+)"/,
      contractFormat: /GeneratedContractFormat\s*=\s*"([^"]+)"/,
      engineApiVersion: /GeneratedEngineAPIVersion\s*=\s*"([^"]+)"/,
      engineVersion: /GeneratedEngineVersion\s*=\s*"([^"]+)"/,
      sdkVersion: /GeneratedSDKVersion\s*=\s*"([^"]+)"/,
    },
    wiring: {
      opsApiVersion: /OpsApiVersion:\s*GeneratedOpenAPIVersion/,
      openApiDigest: /OpenApiDigest:\s*GeneratedOpenAPIDigest/,
      contractFormat: /ContractFormat:\s*GeneratedContractFormat/,
      engineVersion: /EngineVersion:\s*GeneratedEngineVersion/,
      sdkVersion: /SdkVersion:\s*GeneratedSDKVersion/,
    },
  },
  rust: {
    manifestPath: "sdk/rust/Cargo.toml",
    metadataPath: "sdk/rust/src/generated.rs",
    compatibilityPath: "sdk/rust/src/compat.rs",
    // Rust pilot markers omit engineApiVersion today; digest/version still gate.
    requiredMarkers: [
      "opsApiVersion",
      "openApiDigest",
      "contractFormat",
      "engineVersion",
      "sdkVersion",
    ],
    manifest(text) {
      return {
        package: text.match(/^name\s*=\s*"([^"]+)"/m)?.[1],
        sdkVersion: text.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
      };
    },
    markers: {
      opsApiVersion: /GENERATED_OPENAPI_VERSION:\s*&str\s*=\s*"([^"]+)"/,
      openApiDigest: /GENERATED_OPENAPI_DIGEST:\s*&str\s*=\s*"([^"]+)"|GENERATED_OPENAPI_DIGEST:\s*&str\s*=\s*\n\s*"([^"]+)"/,
      contractFormat: /GENERATED_CONTRACT_FORMAT:\s*&str\s*=\s*"([^"]+)"/,
      engineVersion: /GENERATED_ENGINE_VERSION:\s*&str\s*=\s*"([^"]+)"/,
      sdkVersion: /GENERATED_SDK_VERSION:\s*&str\s*=\s*"([^"]+)"/,
    },
    wiring: {
      opsApiVersion: /ops_api_version:\s*GENERATED_OPENAPI_VERSION/,
      openApiDigest: /openapi_digest:\s*GENERATED_OPENAPI_DIGEST/,
      contractFormat: /contract_format:\s*GENERATED_CONTRACT_FORMAT/,
      engineVersion: /engine_version:\s*GENERATED_ENGINE_VERSION/,
      sdkVersion: /sdk_version:\s*GENERATED_SDK_VERSION/,
    },
  },
};

if (!Array.isArray(matrix?.sdks)) fail("matrix.sdks must be a list");
const rows = new Map();
for (const [index, row] of (matrix?.sdks ?? []).entries()) {
  const label = `matrix.sdks[${index}]`;
  for (const field of ["language", "package", "supportedEngineRange", "sdkVersion"]) {
    requireString(row?.[field], `${label}.${field}`);
  }
  if (rows.has(row?.language)) fail(`${label}.language is duplicated: ${row?.language}`);
  rows.set(row?.language, row);
}

expectEqual(
  "matrix SDK languages",
  [...rows.keys()].sort().join(","),
  Object.keys(sdkDefinitions).sort().join(","),
);

for (const [language, definition] of Object.entries(sdkDefinitions)) {
  const row = rows.get(language);
  if (!row) continue;

  const manifest = definition.manifest(read(definition.manifestPath));
  expectEqual(`${language} package`, manifest.package, row.package);
  expectEqual(`${language} manifest sdkVersion`, manifest.sdkVersion, row.sdkVersion);

  const metadata = read(definition.metadataPath);
  const expectedMarkers = {
    opsApiVersion: matrix.opsApiVersion,
    openApiDigest: matrix.openApiDigest,
    contractFormat: matrix.contractFormat,
    engineApiVersion: matrix.engineApiVersion,
    engineVersion: matrix.engineVersion,
    sdkVersion: row.sdkVersion,
  };
  const requiredMarkers = definition.requiredMarkers ?? Object.keys(expectedMarkers);
  for (const field of requiredMarkers) {
    const expected = expectedMarkers[field];
    const pattern = definition.markers[field];
    if (!pattern) {
      fail(`${language} marker pattern is missing: ${field}`);
      continue;
    }
    const matched = metadata.match(pattern);
    const actual = matched?.[1] ?? matched?.[2];
    if (actual === undefined) fail(`${language} generated marker is missing: ${field}`);
    else expectEqual(`${language} generated ${field}`, actual, expected);
  }

  const compatibility = read(definition.compatibilityPath);
  for (const [field, pattern] of Object.entries(definition.wiring)) {
    if (!pattern.test(compatibility)) fail(`${language} compatibility wiring is missing: ${field}`);
  }
}

const report = {
  status: errors.length > 0 ? "fail" : "ok",
  matrixPath,
  specPath,
  sdkCount: rows.size,
  languages: [...rows.keys()].sort(),
  errors,
};
writeOpenApiReport("compat", report);

if (errors.length > 0) {
  console.error(errors.map((error) => `FAIL compatibility: ${error}`).join("\n"));
  process.exit(1);
}

console.log(`compatibility validation ok: ${matrixPath} (${rows.size} SDKs)`);
