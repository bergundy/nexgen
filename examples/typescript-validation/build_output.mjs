import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

function main() {
  const appRoot = dirname(fileURLToPath(import.meta.url));
  const repoRoot = resolve(appRoot, '..', '..');
  const fixtureRoot = resolve(repoRoot, 'tests', 'fixtures', 'sample');
  const inputPath = resolve(fixtureRoot, 'input.yaml');
  const descriptorPath = resolve(repoRoot, 'descriptors.bin');
  const outputPath = resolve(appRoot, 'output.ts');

  const result = spawnSync(
    'cargo',
    [
      'run',
      '--quiet',
      '--',
      'generate',
      '--lang',
      'typescript',
      '--input',
      inputPath,
      '--descriptors',
      descriptorPath,
      '--output',
      outputPath,
    ],
    {
      cwd: repoRoot,
      stdio: 'inherit',
    },
  );

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }

  console.log(`Built ${outputPath} with nexus-api-gen`);
}

main();
