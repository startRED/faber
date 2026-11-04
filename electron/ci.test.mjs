import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('the existing Electron CI job runs every Electron regression suite', () => {
  const { scripts } = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(scripts['test:electron'], 'npm run electron:build && node --test --test-concurrency=2 electron/*.test.mjs');
  const ci = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const job = ci.slice(ci.indexOf('\n  electron:\n'), ci.indexOf('\n  # How long each screen'));
  assert.match(job, /- run: npm run test:electron/);
  assert.ok(job.indexOf('npm run test:electron') > job.indexOf('node node_modules/electron/install.js'));
});
