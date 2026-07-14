#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const specPath = process.argv[2] ?? path.join(repoRoot, 'openapi/templiqx-operations-v1.yaml');
const outDir = path.join(repoRoot, 'target/openapi-sdk-proof');
const generatedPath = path.join(outDir, 'operations-v1.ts');
const tsconfigPath = path.join(outDir, 'tsconfig.json');

fs.mkdirSync(outDir, { recursive: true });

execFileSync(
  'npx',
  ['--yes', 'openapi-typescript@7.10.1', specPath, '-o', generatedPath],
  { cwd: repoRoot, stdio: 'inherit' },
);

fs.writeFileSync(
  tsconfigPath,
  JSON.stringify(
    {
      compilerOptions: {
        target: 'ES2022',
        module: 'NodeNext',
        moduleResolution: 'NodeNext',
        strict: true,
        noEmit: true,
        skipLibCheck: true,
      },
      include: [path.basename(generatedPath)],
    },
    null,
    2,
  ),
);

execFileSync(
  'npx',
  ['--yes', '-p', 'typescript@5.9.3', 'tsc', '--project', tsconfigPath],
  {
    cwd: outDir,
    stdio: 'inherit',
  },
);

console.log(`TypeScript SDK proof ok: ${generatedPath}`);
