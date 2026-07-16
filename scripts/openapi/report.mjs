#!/usr/bin/env node
/**
 * Shared helper: write OpenAPI/compat/bump drift reports under artifacts/openapi/
 * so CI can upload them. Safe to call from gate scripts; never fails the gate itself.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

export function writeOpenApiReport(name, payload) {
  const dir = path.join(repoRoot, "artifacts", "openapi");
  fs.mkdirSync(dir, { recursive: true });
  const body = {
    name,
    generatedAt: new Date().toISOString(),
    ...payload,
  };
  const target = path.join(dir, `${name}.json`);
  fs.writeFileSync(target, `${JSON.stringify(body, null, 2)}\n`, "utf8");
  return target;
}
