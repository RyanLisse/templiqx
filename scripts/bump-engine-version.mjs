#!/usr/bin/env node

import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import YAML from "yaml";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const matrixPath = "openapi/compatibility-matrix.yaml";
const specPath = "openapi/templiqx-operations-v1.yaml";
const cargoManifestPath = "Cargo.toml";
const cargoLockPath = "Cargo.lock";
const changelogPath = "CHANGELOG.md";
const generatedPaths = [
  "sdk/typescript/src/generated/operations-v1.ts",
  "sdk/python/src/templiqx_adapter/_generated/operations_v1.py",
  "sdk/dotnet/Templiqx.Adapter/Generated/OperationsV1.cs",
  "sdk/dotnet/Templiqx.Adapter/Generated/GeneratedMeta.cs",
];
const manifestPaths = {
  typescript: "sdk/typescript/package.json",
  dotnet: "sdk/dotnet/Templiqx.Adapter/Templiqx.Adapter.csproj",
  python: "sdk/python/pyproject.toml",
};
const sdkLockPaths = {
  typescript: "sdk/typescript/package-lock.json",
  python: "sdk/python/uv.lock",
};
const plannedPaths = [
  matrixPath,
  cargoManifestPath,
  cargoLockPath,
  ...Object.values(manifestPaths),
  ...Object.values(sdkLockPaths),
  ...generatedPaths,
  changelogPath,
];
const operationMethods = new Set([
  "get",
  "put",
  "post",
  "delete",
  "options",
  "head",
  "patch",
  "trace",
]);

const absolute = (relativePath) => path.join(repoRoot, relativePath);
const read = (relativePath) => fs.readFileSync(absolute(relativePath), "utf8");
const digest = (text) => `sha256:${createHash("sha256").update(text).digest("hex")}`;

function fail(message) {
  console.error(`FAIL bump engine: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const args = { dryRun: false, yes: false, to: undefined };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--dry-run") args.dryRun = true;
    else if (argument === "--yes") args.yes = true;
    else if (argument === "--to") {
      args.to = argv[index + 1];
      index += 1;
      if (!args.to) fail("--to requires a semantic version");
    } else if (argument === "--help" || argument === "-h") {
      console.log("Usage: just bump-engine [--to <version>] [--dry-run] [--yes]");
      console.log("Without --yes, the command only prints the plan and writes nothing.");
      process.exit(0);
    } else fail(`unknown argument ${JSON.stringify(argument)}`);
  }
  if (args.dryRun && args.yes) fail("--dry-run and --yes cannot be used together");
  return args;
}

function parseVersion(value, label) {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(value ?? "");
  if (!match) fail(`${label} must be a stable semantic version in major.minor.patch form`);
  return { major: Number(match[1]), minor: Number(match[2]), patch: Number(match[3]), value };
}

function compareVersions(left, right) {
  for (const field of ["major", "minor", "patch"]) {
    if (left[field] !== right[field]) return left[field] - right[field];
  }
  return 0;
}

function bumpedVersion(version, level) {
  if (level === "minor") return `${version.major}.${version.minor + 1}.0`;
  return `${version.major}.${version.minor}.${version.patch + 1}`;
}

function operationIds(document, label) {
  const ids = new Set();
  for (const [route, pathItem] of Object.entries(document?.paths ?? {})) {
    if (!pathItem || typeof pathItem !== "object") continue;
    for (const [method, operation] of Object.entries(pathItem)) {
      if (!operationMethods.has(method) || !operation || typeof operation !== "object") continue;
      if (typeof operation.operationId !== "string" || operation.operationId.length === 0) {
        fail(`${label} ${method.toUpperCase()} ${route} has no operationId`);
      }
      if (ids.has(operation.operationId)) fail(`${label} repeats operationId ${operation.operationId}`);
      ids.add(operation.operationId);
    }
  }
  return ids;
}

function gitShow(relativePath) {
  try {
    return execFileSync("git", ["show", `HEAD:${relativePath}`], {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
  } catch (error) {
    const detail = error.stderr?.trim();
    fail(`could not read git HEAD version of ${relativePath}${detail ? `: ${detail}` : ""}`);
  }
}

function replaceExactlyOnce(text, pattern, replacement, label) {
  const matches = text.match(new RegExp(pattern.source, pattern.flags.includes("g") ? pattern.flags : `${pattern.flags}g`));
  if (matches?.length !== 1) fail(`${label}: expected exactly one matching version field`);
  return text.replace(pattern, replacement);
}

function replaceYamlScalars(text, field, value, expectedCount) {
  const pattern = new RegExp(`^(\\s*${field}:\\s*)(.*)$`, "gm");
  let count = 0;
  const next = text.replace(pattern, (_line, prefix, previous) => {
    count += 1;
    const trimmed = previous.trim();
    const formatted = trimmed.startsWith('"') ? JSON.stringify(value) : value;
    return `${prefix}${formatted}`;
  });
  if (count !== expectedCount) {
    fail(`${matrixPath}: expected ${expectedCount} ${field} field${expectedCount === 1 ? "" : "s"}, got ${count}`);
  }
  return next;
}

function writeMatrix(
  matrix,
  nextVersion,
  nextEngineApiVersion,
  nextDigest,
  nextOpsApiVersion,
  nextContractFormat,
) {
  const parsed = parseVersion(nextVersion, "target version");
  const supportedEngineRange = `>=${parsed.major}.${parsed.minor}.0,<${parsed.major}.${parsed.minor + 1}.0`;
  let next = read(matrixPath);
  next = replaceYamlScalars(next, "opsApiVersion", nextOpsApiVersion, 1);
  next = replaceYamlScalars(next, "openApiDigest", nextDigest, 1);
  next = replaceYamlScalars(next, "contractFormat", nextContractFormat, 1);
  next = replaceYamlScalars(next, "engineApiVersion", nextEngineApiVersion, 1);
  next = replaceYamlScalars(next, "engineVersion", nextVersion, 1);
  next = replaceYamlScalars(next, "supportedEngineRange", supportedEngineRange, matrix.sdks.length);
  next = replaceYamlScalars(next, "sdkVersion", nextVersion, matrix.sdks.length);
  fs.writeFileSync(absolute(matrixPath), next);
}

function writeManifests(nextVersion) {
  const cargo = replaceExactlyOnce(
    read(cargoManifestPath),
    /(\[workspace\.package\][\s\S]*?^version\s*=\s*")[^"]+("\s*$)/m,
    `$1${nextVersion}$2`,
    cargoManifestPath,
  );
  fs.writeFileSync(absolute(cargoManifestPath), cargo);

  const typescript = JSON.parse(read(manifestPaths.typescript));
  typescript.version = nextVersion;
  fs.writeFileSync(absolute(manifestPaths.typescript), `${JSON.stringify(typescript, null, 2)}\n`);

  const typescriptLock = JSON.parse(read(sdkLockPaths.typescript));
  typescriptLock.version = nextVersion;
  typescriptLock.packages[""].version = nextVersion;
  fs.writeFileSync(absolute(sdkLockPaths.typescript), `${JSON.stringify(typescriptLock, null, 2)}\n`);

  const dotnet = replaceExactlyOnce(
    read(manifestPaths.dotnet),
    /(<Version>)[^<]+(<\/Version>)/,
    `$1${nextVersion}$2`,
    manifestPaths.dotnet,
  );
  fs.writeFileSync(absolute(manifestPaths.dotnet), dotnet);

  const python = replaceExactlyOnce(
    read(manifestPaths.python),
    /(\[project\][\s\S]*?^version\s*=\s*")[^"]+("\s*$)/m,
    `$1${nextVersion}$2`,
    manifestPaths.python,
  );
  fs.writeFileSync(absolute(manifestPaths.python), python);

  const pythonLock = replaceExactlyOnce(
    read(sdkLockPaths.python),
    /(\[\[package\]\]\nname = "templiqx-adapter"\nversion = ")[^"]+("\s*$)/m,
    `$1${nextVersion}$2`,
    sdkLockPaths.python,
  );
  fs.writeFileSync(absolute(sdkLockPaths.python), pythonLock);
}

function run(command, args, label, quietStdout = false) {
  console.log(`Running ${label}...`);
  const stdio = quietStdout ? ["ignore", "ignore", "inherit"] : "inherit";
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8", stdio });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${label} failed with exit code ${result.status}`);
}

function workspacePackageVersions() {
  const result = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps", "--offline"],
    { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`cargo metadata failed: ${result.stderr.trim()}`);
  const metadata = JSON.parse(result.stdout);
  const workspaceMembers = new Set(metadata.workspace_members);
  return new Map(
    metadata.packages
      .filter((pkg) => workspaceMembers.has(pkg.id))
      .map((pkg) => [pkg.name, pkg.version]),
  );
}

function localCargoLockVersions(lockText) {
  const versions = new Map();
  for (const block of lockText.match(/\[\[package\]\]\n[\s\S]*?(?=\n\[\[package\]\]|$)/g) ?? []) {
    if (/^source = /m.test(block)) continue;
    const name = block.match(/^name = "([^"]+)"$/m)?.[1];
    const version = block.match(/^version = "([^"]+)"$/m)?.[1];
    if (name && version) versions.set(name, version);
  }
  return versions;
}

function updateCargoLock() {
  console.log("Running Cargo lockfile refresh...");
  const expected = workspacePackageVersions();
  const seen = new Set();
  const next = read(cargoLockPath).replace(
    /\[\[package\]\]\n[\s\S]*?(?=\n\[\[package\]\]|$)/g,
    (block) => {
      if (/^source = /m.test(block)) return block;
      const name = block.match(/^name = "([^"]+)"$/m)?.[1];
      if (!name || !expected.has(name)) return block;
      seen.add(name);
      return block.replace(/^version = "[^"]+"$/m, `version = "${expected.get(name)}"`);
    },
  );
  const missing = [...expected.keys()].filter((name) => !seen.has(name));
  if (missing.length > 0) throw new Error(`Cargo.lock is missing workspace packages: ${missing.join(", ")}`);
  fs.writeFileSync(absolute(cargoLockPath), next);
}

function runGenerators() {
  run(process.execPath, ["sdk/typescript/scripts/generate.mjs"], "TypeScript SDK generator");
  run(
    "uv",
    ["run", "--offline", "--project", "sdk/python", "python", "sdk/python/scripts/generate.py"],
    "Python SDK generator",
  );
  run("sdk/dotnet/scripts/generate.sh", [], ".NET SDK generator");
}

function changelogDetails(classification, added, oldDigest, nextDigest) {
  if (classification === "patch") return "Engine and compatibility metadata changed; the Operations OpenAPI digest is unchanged.";
  if (added.length > 0) {
    const formatted = added.map((id) => `\`${id}\``).join(", ");
    return `The Operations API changed additively with new operation IDs ${formatted}; no operation IDs were removed.`;
  }
  return `The Operations OpenAPI digest changed from \`${oldDigest}\` to \`${nextDigest}\` without operation removals.`;
}

function writeChangelog(oldVersion, nextVersion, nextEngineApiVersion, classification, added, oldDigest, nextDigest) {
  const current = read(changelogPath);
  const heading = `## [${nextVersion}] - Unreleased`;
  if (current.includes(heading)) return;
  const stanza = [
    heading,
    "",
    "### Changed",
    "",
    `- Coordinated the engine and all SDK packages from \`${oldVersion}\` to \`${nextVersion}\` with engine API line \`${nextEngineApiVersion}\`.`,
    `- ${changelogDetails(classification, added, oldDigest, nextDigest)}`,
    "",
  ].join("\n");
  const nextHeading = current.indexOf("\n## [", current.indexOf("## [Unreleased]") + 1);
  if (nextHeading < 0) fail(`${changelogPath}: could not find the first released-version heading`);
  let next = `${current.slice(0, nextHeading + 1)}${stanza}\n${current.slice(nextHeading + 1)}`;
  next = replaceExactlyOnce(
    next,
    /^\[Unreleased\]: .*$/m,
    `[Unreleased]: https://github.com/RyanLisse/templiqx/compare/v${nextVersion}...HEAD`,
    `${changelogPath} Unreleased link`,
  );
  const oldLinkIndex = next.indexOf(`[${oldVersion}]:`);
  if (oldLinkIndex < 0) fail(`${changelogPath}: could not find the ${oldVersion} link definition`);
  const link = `[${nextVersion}]: https://github.com/RyanLisse/templiqx/compare/v${oldVersion}...v${nextVersion}\n`;
  next = `${next.slice(0, oldLinkIndex)}${link}${next.slice(oldLinkIndex)}`;
  fs.writeFileSync(absolute(changelogPath), next);
}

function readManifestVersions() {
  const typescript = JSON.parse(read(manifestPaths.typescript));
  const typescriptLock = JSON.parse(read(sdkLockPaths.typescript));
  const dotnet = read(manifestPaths.dotnet).match(/<Version>([^<]+)<\/Version>/)?.[1];
  const pythonProject = read(manifestPaths.python).match(/\[project\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
  const pythonLock = read(sdkLockPaths.python).match(
    /\[\[package\]\]\nname = "templiqx-adapter"\nversion = "([^"]+)"/,
  )?.[1];
  const cargo = read(cargoManifestPath).match(
    /\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m,
  )?.[1];
  return [
    cargo,
    typescript.version,
    typescriptLock.version,
    typescriptLock.packages[""].version,
    dotnet,
    pythonProject,
    pythonLock,
  ];
}

function generatedMetadataMatches(
  version,
  engineApiVersion,
  openApiDigest,
  opsApiVersion,
  contractFormat,
) {
  const markers = [
    read(generatedPaths[0]).includes(`GENERATED_OPENAPI_VERSION = "${opsApiVersion}"`) &&
      read(generatedPaths[0]).includes(`GENERATED_CONTRACT_FORMAT = "${contractFormat}"`) &&
    read(generatedPaths[0]).includes(`GENERATED_ENGINE_VERSION = "${version}"`) &&
      read(generatedPaths[0]).includes(`GENERATED_SDK_VERSION = "${version}"`) &&
      read(generatedPaths[0]).includes(`GENERATED_ENGINE_API_VERSION = "${engineApiVersion}"`) &&
      read(generatedPaths[0]).includes(`GENERATED_OPENAPI_DIGEST = "${openApiDigest}"`),
    read(generatedPaths[1]).includes(`GENERATED_OPENAPI_VERSION = '${opsApiVersion}'`) &&
      read(generatedPaths[1]).includes(`GENERATED_CONTRACT_FORMAT = '${contractFormat}'`) &&
      read(generatedPaths[1]).includes(`GENERATED_ENGINE_VERSION = '${version}'`) &&
      read(generatedPaths[1]).includes(`GENERATED_SDK_VERSION = '${version}'`) &&
      read(generatedPaths[1]).includes(`GENERATED_ENGINE_API_VERSION = '${engineApiVersion}'`) &&
      read(generatedPaths[1]).includes(`GENERATED_OPENAPI_DIGEST = '${openApiDigest}'`),
    read(generatedPaths[3]).includes(`GeneratedOpenApiVersion = "${opsApiVersion}"`) &&
      read(generatedPaths[3]).includes(`GeneratedContractFormat = "${contractFormat}"`) &&
      read(generatedPaths[3]).includes(`GeneratedEngineVersion = "${version}"`) &&
      read(generatedPaths[3]).includes(`GeneratedSdkVersion = "${version}"`) &&
      read(generatedPaths[3]).includes(`GeneratedEngineApiVersion = "${engineApiVersion}"`) &&
      read(generatedPaths[3]).includes(`GeneratedOpenApiDigest = "${openApiDigest}"`),
  ];
  return markers.every(Boolean);
}

function cargoLockMatches() {
  try {
    const expected = workspacePackageVersions();
    const actual = localCargoLockVersions(read(cargoLockPath));
    return [...expected].every(([name, version]) => actual.get(name) === version);
  } catch {
    return false;
  }
}

function printPlan(plan, applied) {
  console.log("Engine version bump plan");
  console.log(`Classification: ${plan.classification}`);
  console.log(`Proposed bump level: ${plan.level}`);
  console.log(`Version: ${plan.baselineVersion} -> ${plan.nextVersion}${plan.override ? " (--to override)" : ""}`);
  console.log(`Engine API version: ${plan.currentEngineApiVersion} -> ${plan.nextEngineApiVersion}`);
  console.log(`Operations API version: ${plan.currentOpsApiVersion} -> ${plan.nextOpsApiVersion}`);
  console.log(`Contract format: ${plan.currentContractFormat} -> ${plan.nextContractFormat}`);
  console.log(`OpenAPI digest: ${plan.matrixDigest} -> ${plan.nextDigest}`);
  console.log(`Operation IDs added: ${plan.added.length > 0 ? plan.added.join(", ") : "none"}`);
  console.log(`Operation IDs removed: ${plan.removed.length > 0 ? plan.removed.join(", ") : "none"}`);
  console.log("Version-controlled files the command will touch:");
  for (const relativePath of plannedPaths) console.log(`  - ${relativePath}`);
  if (applied) return;
  console.log("Mode: dry run; no files written.");
  const override = plan.override ? ` --to ${plan.nextVersion}` : "";
  console.log(`Apply with: just bump-engine${override} --yes`);
}

const args = parseArgs(process.argv.slice(2));
const specText = read(specPath);
const headSpecText = gitShow(specPath);
const matrixText = read(matrixPath);
const matrix = YAML.parse(matrixText, { prettyErrors: true });
const headMatrix = YAML.parse(gitShow(matrixPath), { prettyErrors: true });
if (!Array.isArray(matrix?.sdks) || matrix.sdks.length === 0) fail(`${matrixPath} has no SDK rows`);

const spec = YAML.parse(specText, { prettyErrors: true });
const currentIds = operationIds(spec, "working OpenAPI");
const headIds = operationIds(YAML.parse(headSpecText, { prettyErrors: true }), "HEAD OpenAPI");
const added = [...currentIds].filter((id) => !headIds.has(id)).sort();
const removed = [...headIds].filter((id) => !currentIds.has(id)).sort();
const nextDigest = digest(specText);
const headDigest = digest(headSpecText);
const nextOpsApiVersion = spec?.info?.version;
const nextContractFormat =
  spec?.components?.schemas?.OperationEnvelopeBase?.properties?.api_version?.const;
if (typeof nextOpsApiVersion !== "string" || nextOpsApiVersion.length === 0) {
  fail(`${specPath} has no info.version`);
}
if (typeof nextContractFormat !== "string" || nextContractFormat.length === 0) {
  fail(`${specPath} has no OperationEnvelopeBase api_version const`);
}
const classification = removed.length > 0 ? "breaking" : added.length > 0 || nextDigest !== headDigest ? "additive" : "patch";
const level = classification === "additive" ? "minor" : classification;

if (classification === "breaking") {
  console.error("Engine version bump plan");
  console.error("Classification: breaking");
  console.error(`Operation IDs removed or renamed: ${removed.join(", ")}`);
  console.error("REFUSED: an in-place bump cannot carry a breaking Operations API change.");
  console.error("Introduce a new /operations/vN base path and OpenAPI document, then perform the corresponding major bump.");
  process.exit(1);
}

const baseline = parseVersion(headMatrix?.engineVersion, "HEAD matrix.engineVersion");
const working = parseVersion(matrix?.engineVersion, "matrix.engineVersion");
const nextVersion = args.to ?? bumpedVersion(baseline, level);
const target = parseVersion(nextVersion, "target version");
if (compareVersions(target, baseline) <= 0) fail(`target version ${nextVersion} must be greater than HEAD version ${baseline.value}`);
if (
  classification === "additive" &&
  target.major === baseline.major &&
  target.minor === baseline.minor
) {
  fail(`additive Operations API changes require at least a minor bump from ${baseline.value}`);
}
const nextEngineApiVersion = `${target.major}.${target.minor}`;
const plan = {
  added,
  baselineVersion: baseline.value,
  classification,
  currentContractFormat: matrix.contractFormat,
  currentEngineApiVersion: matrix.engineApiVersion,
  currentOpsApiVersion: matrix.opsApiVersion,
  level,
  matrixDigest: matrix.openApiDigest,
  nextDigest,
  nextContractFormat,
  nextEngineApiVersion,
  nextOpsApiVersion,
  nextVersion,
  override: args.to !== undefined,
  removed,
};

if (working.value !== baseline.value && working.value !== nextVersion) {
  fail(`working matrix version ${working.value} is neither HEAD ${baseline.value} nor target ${nextVersion}`);
}

const alreadyApplied =
  working.value === nextVersion &&
  matrix.opsApiVersion === nextOpsApiVersion &&
  matrix.openApiDigest === nextDigest &&
  matrix.contractFormat === nextContractFormat &&
  matrix.engineApiVersion === nextEngineApiVersion &&
  matrix.sdks.every(
    (sdk) =>
      sdk.sdkVersion === nextVersion &&
      sdk.supportedEngineRange ===
        `>=${target.major}.${target.minor}.0,<${target.major}.${target.minor + 1}.0`,
  ) &&
  readManifestVersions().every((version) => version === nextVersion) &&
  cargoLockMatches() &&
  generatedMetadataMatches(
    nextVersion,
    nextEngineApiVersion,
    nextDigest,
    nextOpsApiVersion,
    nextContractFormat,
  ) &&
  read(changelogPath).includes(`## [${nextVersion}] - Unreleased`);
if (alreadyApplied) {
  printPlan(plan, true);
  console.log(`Already applied: version ${nextVersion} and digest ${nextDigest} are current.`);
  process.exit(0);
}

if (!args.yes) {
  printPlan(plan, false);
  process.exit(0);
}

printPlan(plan, true);
const originals = new Map(plannedPaths.map((relativePath) => [relativePath, fs.existsSync(absolute(relativePath)) ? read(relativePath) : undefined]));
try {
  writeMatrix(
    matrix,
    nextVersion,
    nextEngineApiVersion,
    nextDigest,
    nextOpsApiVersion,
    nextContractFormat,
  );
  writeManifests(nextVersion);
  updateCargoLock();
  runGenerators();
  writeChangelog(baseline.value, nextVersion, nextEngineApiVersion, classification, added, headDigest, nextDigest);
} catch (error) {
  for (const [relativePath, content] of originals) {
    if (content === undefined) fs.rmSync(absolute(relativePath), { force: true });
    else fs.writeFileSync(absolute(relativePath), content);
  }
  fail(`apply failed and version-controlled files were restored: ${error.message}`);
}

console.log(`Applied coordinated engine and SDK bump to ${nextVersion}.`);
console.log("Review the ordinary git diff; no commit or publish was performed.");
