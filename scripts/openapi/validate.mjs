#!/usr/bin/env node
import fs from 'node:fs';
import YAML from 'yaml';
import { writeOpenApiReport } from './report.mjs';

const specPath = process.argv[2] ?? 'openapi/templiqx-operations-v1.yaml';
const raw = fs.readFileSync(specPath, 'utf8');
const spec = YAML.parse(raw, { prettyErrors: true });
const errors = [];
const fail = (msg) => errors.push(msg);
if (spec?.openapi !== '3.1.0') fail('openapi must be 3.1.0');
if (!spec?.info?.title || !spec?.info?.version) fail('info.title and info.version are required');
if (!spec.paths || Object.keys(spec.paths).length === 0) fail('paths are required');
if (!spec.components?.schemas) fail('components.schemas are required');

const refs = new Set();
function walk(value, path = '$') {
  if (Array.isArray(value)) return value.forEach((item, i) => walk(item, `${path}[${i}]`));
  if (!value || typeof value !== 'object') return;
  if (typeof value.$ref === 'string') refs.add(`${path} -> ${value.$ref}`);
  for (const [key, child] of Object.entries(value)) walk(child, `${path}.${key}`);
}
walk(spec);
function resolveRef(pointer) {
  if (!pointer.startsWith('#/')) return undefined;
  return pointer
    .slice(2)
    .split('/')
    .reduce(
      (node, part) => node?.[part.replaceAll('~1', '/').replaceAll('~0', '~')],
      spec,
    );
}
for (const ref of refs) {
  const [, pointer] = ref.split(' -> ');
  if (!pointer.startsWith('#/')) fail(`external or invalid ref disallowed: ${ref}`);
  if (resolveRef(pointer) === undefined) fail(`unresolved ref: ${ref}`);
}

const operationIds = new Map();
for (const [path, pathItem] of Object.entries(spec.paths ?? {})) {
  if (!path.startsWith('/operations/v1/')) fail(`path must be versioned under /operations/v1/: ${path}`);
  for (const method of ['get', 'post', 'put', 'patch', 'delete']) {
    const op = pathItem?.[method];
    if (!op) continue;
    if (!op.operationId) fail(`${method.toUpperCase()} ${path} missing operationId`);
    if (operationIds.has(op.operationId)) fail(`duplicate operationId: ${op.operationId}`);
    operationIds.set(op.operationId, `${method.toUpperCase()} ${path}`);
    const response = op.responses?.['200'] ?? op.responses?.['201'] ?? op.responses?.['202'];
    if (!response) fail(`${method.toUpperCase()} ${path} missing 2xx response`);
    const resolved = response?.$ref ? resolveRef(response.$ref) : response;
    const hasJsonSchema = Boolean(resolved?.content?.['application/json']?.schema);
    const hasYamlSchema = Boolean(resolved?.content?.['application/yaml']?.schema);
    const openApiDocument =
      op.operationId === 'getOperationsV1OpenApi' || op.operationId === 'getOperationsV1OpenApiYaml';
    if (!hasJsonSchema && !(openApiDocument && hasYamlSchema)) {
      fail(`${method.toUpperCase()} ${path} missing supported response schema`);
    }
    if (method !== 'get' && op['x-templiqx-idempotent'] === undefined) fail(`${method.toUpperCase()} ${path} must declare x-templiqx-idempotent`);
  }
}
for (const required of ['catalog', 'discoverPackages', 'inspectContract', 'validateContract', 'compileContract', 'executeContract']) {
  if (!operationIds.has(required)) fail(`missing required operationId: ${required}`);
}
for (const schema of ['Diagnostic', 'OperationEnvelopeBase', 'StringListEnvelope', 'JsonValueEnvelope']) {
  if (!spec.components.schemas[schema]) fail(`missing required schema: ${schema}`);
}

const qualityIntegerFields = {
  'EligibilityRule.threshold': [0, 9007199254740991],
  'QualityPolicy.replicates_per_fixture': [1, 20],
  'QualityPolicy.minimum_semantic_cases': [0, 9007199254740991],
  'QualityPolicy.maximum_infrastructure_failure_ppm': [0, 1000000],
  'TrialEvidence.replicate_index': [0, 65535],
  'TrialEvidence.provider_attempt_count': [1, 4294967295],
  'MetricObservation.value': [0, 9007199254740991],
  'MetricAggregate.value': [0, 9007199254740991],
  'EligibilityGate.actual': [0, 9007199254740991],
  'EligibilityGate.threshold': [0, 9007199254740991],
  'EligibilityAssessment.total_trial_count': [0, 9007199254740991],
  'EligibilityAssessment.semantic_trial_count': [0, 9007199254740991],
  'EligibilityAssessment.infrastructure_trial_count': [0, 9007199254740991],
  'EligibilityAssessment.semantic_coverage_ppm': [0, 1000000],
  'EligibilityAssessment.infrastructure_failure_ppm': [0, 1000000],
  'QualityTrialSummary.replicate_index': [0, 65535],
  'QualityTrialSummary.provider_attempt_count': [0, 4294967295],
  'ParetoFront.rank': [0, 4294967295],
};
for (const [qualifiedName, [minimum, maximum]] of Object.entries(qualityIntegerFields)) {
  const [schemaName, propertyName] = qualifiedName.split('.');
  const property = spec.components.schemas[schemaName]?.properties?.[propertyName];
  if (property?.type !== 'integer') fail(`${qualifiedName} must be a JSON integer`);
  if (property?.format !== 'int64') fail(`${qualifiedName} must use format int64`);
  if (property?.minimum !== minimum) fail(`${qualifiedName} must have minimum ${minimum}`);
  if (property?.maximum !== maximum) fail(`${qualifiedName} must have maximum ${maximum}`);
}
const claimedIdentityNames = [
  'claimed_candidate_contract_fingerprint',
  'claimed_evaluator_profile_fingerprint',
  'claimed_measurement_profile_fingerprints',
  'claimed_model_profile_fingerprint',
  'claimed_scorer_fingerprints',
];
const claimedIdentities = spec.components.schemas.ClaimedQualityIdentities;
if ([...(claimedIdentities?.required ?? [])].sort().join(',') !== claimedIdentityNames.join(',')) {
  fail('ClaimedQualityIdentities required fields must use explicit claimed_* names');
}
if (Object.keys(claimedIdentities?.properties ?? {}).sort().join(',') !== claimedIdentityNames.join(',')) {
  fail('ClaimedQualityIdentities properties must use explicit claimed_* names');
}
const candidateAssessment = spec.components.schemas.CandidateAssessment;
if ((candidateAssessment?.required ?? []).includes('claimed_identities')) {
  fail('CandidateAssessment.claimed_identities must remain optional for invalid claims');
}
if (candidateAssessment?.properties?.claimed_identities?.$ref !== '#/components/schemas/ClaimedQualityIdentities') {
  fail('CandidateAssessment.claimed_identities must reference ClaimedQualityIdentities');
}
const report = {
  status: errors.length ? 'fail' : 'ok',
  specPath,
  operationCount: operationIds.size,
  errors,
};
writeOpenApiReport('validate', report);

if (errors.length) {
  console.error(errors.map((e) => `FAIL openapi: ${e}`).join('\n'));
  process.exit(1);
}
console.log(`openapi validation ok: ${specPath} (${operationIds.size} operations)`);
