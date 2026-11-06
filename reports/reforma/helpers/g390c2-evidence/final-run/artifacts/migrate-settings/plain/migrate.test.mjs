import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('./migrate.mjs', import.meta.url));
const directory = mkdtempSync(join(tmpdir(), 'migrate-test-'));
const file = join(directory, 'settings.json');
const run = (...args) => spawnSync(process.execPath, [script, ...args]);

try {
  const original = {
    version: 1, theme: 'dark', zoom: 0,
    preferences: { enabled: false, nested: [null, { unknown: 'kept' }] },
  };
  writeFileSync(file, JSON.stringify(original));
  assert.equal(run(file).status, 0);
  const { theme, ...rest } = original;
  assert.deepEqual(JSON.parse(readFileSync(file, 'utf8')), {
    ...rest, version: 2, appearance: { theme },
  });
  const migrated = readFileSync(file);
  assert.equal(run(file).status, 0);
  assert.deepEqual(readFileSync(file), migrated);

  const unchanged = [
    [' \r\n{"version":2,"unknown":{"theme":false}}\t', true],
    ['{"version":1,"theme":""}', true],
    ['{"version":', false],
    ['{"version":3,"theme":"dark"}', false],
    ['{"version":"1","theme":"dark"}', false],
    ['{"version":1}', false],
    ...[null, false, 0, [], {}].map(theme => [JSON.stringify({ version: 1, theme }), false]),
    ['null', false],
    ['[]', false],
  ];
  for (const [input, valid] of unchanged) {
    writeFileSync(file, input);
    const before = readFileSync(file);
    const result = run(file);
    assert.equal(result.status === 0, valid, input);
    if (!valid || JSON.parse(input)?.version === 2) {
      assert.deepEqual(readFileSync(file), before, input);
    }
  }
  assert.notEqual(run().status, 0);
  assert.notEqual(run(join(directory, 'missing.json')).status, 0);
  console.log('All migration checks passed.');
} finally {
  rmSync(directory, { recursive: true, force: true });
}
