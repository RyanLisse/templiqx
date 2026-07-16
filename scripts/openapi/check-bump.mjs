#!/usr/bin/env node

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import YAML from "yaml";
import { writeOpenApiReport } from "./report.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const matrixPath = "openapi/compatibility-matrix.yaml";
const specPath = "openapi/templiqx-operations-v1.yaml";
const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const matrix = YAML.parse(read(matrixPath), { prettyErrors: true });
const actualDigest = `sha256:${createHash("sha256").update(read(specPath)).digest("hex")}`;
const matched = matrix?.openApiDigest === actualDigest;

writeOpenApiReport("bump", {
  status: matched ? "ok" : "fail",
  specPath,
  matrixPath,
  actualDigest,
  matrixDigest: matrix?.openApiDigest ?? null,
  remediation: matched ? null : "just bump-engine",
});

if (!matched) {
  console.error(`FAIL bump check: ${specPath} digest ${actualDigest} does not match ${matrixPath} digest ${matrix?.openApiDigest}.`);
  console.error("Run exactly: just bump-engine");
  process.exit(1);
}

console.log(`bump check ok: ${specPath} matches ${matrixPath}`);
