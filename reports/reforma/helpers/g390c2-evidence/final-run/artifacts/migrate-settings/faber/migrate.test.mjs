import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const directory = mkdtempSync(join(tmpdir(), 'settings-migration-'));
const path = join(directory, 'settings.json');
const script = fileURLToPath(new URL('./migrate.mjs', import.meta.url));
const run = (...args) => spawnSync(process.execPath, [script, ...args]);
try {
  const original = {
    version: 1, theme: '', zoom: 0, enabled: false,
    preferences: { unknown: [null, { theme: 'nested', value: '雪' }] },
  };
  writeFileSync(path, JSON.stringify(original));
  assert.equal(run(path).status, 0);
  const { theme, ...rest } = original;
  assert.deepEqual(JSON.parse(readFileSync(path, 'utf8')), {
    ...rest, version: 2, appearance: { theme },
  });
  const migrated = readFileSync(path);
  assert.equal(run(path).status, 0);
  assert.deepEqual(readFileSync(path), migrated);

  for (const input of [
    ' \r\n{ "version": 2, "unknown": [1, false] }\t',
    '{"version":2,"huge":9007199254740993}',
  ]) {
    writeFileSync(path, input);
    assert.equal(run(path).status, 0);
    assert.deepEqual(readFileSync(path), Buffer.from(input));
  }
  for (const input of [
    '{', 'null', '[]', '{}', '{"version":3}', '{"version":"1","theme":"dark"}',
    '{"version":1}', '{"version":1,"theme":null}',
    '{"version":1,"theme":42}', '{"version":1,"theme":{}}',
  ]) {
    writeFileSync(path, input);
    assert.notEqual(run(path).status, 0);
    assert.deepEqual(readFileSync(path), Buffer.from(input));
  }
  assert.notEqual(run().status, 0);
  assert.notEqual(run(join(directory, 'missing.json')).status, 0);
  console.log('All migration assertions passed');
} finally {
  rmSync(directory, { recursive: true, force: true });
}
