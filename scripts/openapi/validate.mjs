#!/usr/bin/env node
import fs from 'node:fs';
import YAML from 'yaml';

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
for (const ref of refs) {
  const [, pointer] = ref.split(' -> ');
  if (!pointer.startsWith('#/')) fail(`external or invalid ref disallowed: ${ref}`);
  const found = pointer.slice(2).split('/').reduce((node, part) => node?.[part.replaceAll('~1', '/').replaceAll('~0', '~')], spec);
  if (found === undefined) fail(`unresolved ref: ${ref}`);
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
    const hasJsonSchema = Boolean(response?.content?.['application/json']?.schema);
    const hasYamlSchema = Boolean(response?.content?.['application/yaml']?.schema);
    if (!hasJsonSchema && !(op.operationId === 'getOperationsV1OpenApi' && hasYamlSchema)) {
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
if (errors.length) {
  console.error(errors.map((e) => `FAIL openapi: ${e}`).join('\n'));
  process.exit(1);
}
console.log(`openapi validation ok: ${specPath} (${operationIds.size} operations)`);
