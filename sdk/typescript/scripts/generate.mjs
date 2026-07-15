#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import YAML from "yaml";

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(packageRoot, "../..");
const specPath = path.join(repoRoot, "openapi/templiqx-operations-v1.yaml");
const matrixPath = path.join(repoRoot, "openapi/compatibility-matrix.yaml");
const outputPath = path.join(packageRoot, "src/generated/operations-v1.ts");
const packageJson = JSON.parse(fs.readFileSync(path.join(packageRoot, "package.json"), "utf8"));
const spec = fs.readFileSync(specPath, "utf8");
const matrix = YAML.parse(fs.readFileSync(matrixPath, "utf8"), { prettyErrors: true });
const version = spec.match(/^  version:\s*([^\s#]+)\s*$/m)?.[1];
const sdk = matrix?.sdks?.find((candidate) => candidate.language === "typescript");

if (!version || !sdk) {
  throw new Error(`Could not read the OpenAPI version or TypeScript SDK compatibility from ${matrixPath}`);
}
if (sdk.package !== packageJson.name || sdk.sdkVersion !== packageJson.version) {
  throw new Error("TypeScript package metadata does not match the compatibility matrix");
}

const digest = `sha256:${createHash("sha256").update(spec).digest("hex")}`;
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "templiqx-ts-sdk-"));
const tempOutput = path.join(tempDir, "operations-v1.ts");

try {
  const cli = path.join(packageRoot, "node_modules/openapi-typescript/bin/cli.js");
  execFileSync(process.execPath, [cli, specPath, "-o", tempOutput], {
    cwd: packageRoot,
    stdio: "inherit",
  });

  const generated = fs.readFileSync(tempOutput, "utf8");
  const metadata = [
    "",
    "/** Codegen metadata used by the compatibility self-check. */",
    `export const GENERATED_OPENAPI_VERSION = ${JSON.stringify(version)};`,
    `export const GENERATED_OPENAPI_DIGEST = ${JSON.stringify(digest)};`,
    `export const GENERATED_CONTRACT_FORMAT = ${JSON.stringify(matrix.contractFormat)};`,
    `export const GENERATED_ENGINE_API_VERSION = ${JSON.stringify(matrix.engineApiVersion)};`,
    `export const GENERATED_ENGINE_VERSION = ${JSON.stringify(matrix.engineVersion)};`,
    `export const GENERATED_SDK_VERSION = ${JSON.stringify(sdk.sdkVersion)};`,
    "",
  ].join("\n");
  const next = generated + metadata;

  if (process.argv.includes("--check")) {
    const current = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8") : "";
    if (current !== next) {
      console.error(`Generated TypeScript DTOs are stale: ${outputPath}`);
      process.exitCode = 1;
    } else {
      console.log(`Generated TypeScript DTOs are current: ${outputPath}`);
    }
  } else {
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, next);
    console.log(`Generated TypeScript DTOs: ${outputPath}`);
  }
} finally {
  fs.rmSync(tempDir, { recursive: true, force: true });
}
