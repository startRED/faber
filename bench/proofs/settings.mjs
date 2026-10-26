import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const [folder, pristine] = process.argv.slice(2), checks = [];
const check = (name, run) => {
  try { run(); checks.push({ name, ok: true }); }
  catch (error) { checks.push({ name, ok: false, detail: String(error.message).slice(0, 500) }); }
};
const migrate = path => spawnSync(process.execPath, [resolve(folder, 'migrate.mjs'), path], { cwd: folder, windowsHide: true, encoding: 'utf8', timeout: 5000 });
check('notes preserved', () => assert.deepEqual(readFileSync(join(folder, 'notes.txt')), readFileSync(join(pristine, 'notes.txt'))));
check('supplied settings migrated without data loss', () => {
  const { theme, ...rest } = JSON.parse(readFileSync(join(pristine, 'settings.json'), 'utf8'));
  assert.deepEqual(JSON.parse(readFileSync(join(folder, 'settings.json'), 'utf8')), { ...rest, version: 2, appearance: { theme } });
});
const temp = mkdtempSync(join(tmpdir(), 'faber-migrate-proof-'));
try {
  check('fresh preferences preserved and migration is idempotent', () => {
    const path = join(temp, 'preferences.json');
    const value = { version: 1, theme: 'light', language: 'pt-BR', custom: { nested: [0, false, 'á'] } };
    writeFileSync(path, JSON.stringify(value));
    assert.equal(migrate(path).status, 0);
    const bytes = readFileSync(path);
    const { theme, ...rest } = value;
    assert.deepEqual(JSON.parse(bytes), { ...rest, version: 2, appearance: { theme } });
    assert.equal(migrate(path).status, 0);
    assert.deepEqual(readFileSync(path), bytes);
    const existing = '{ "version": 2, "appearance": {"theme":"dark"}, "other": true }\n';
    writeFileSync(path, existing);
    assert.equal(migrate(path).status, 0);
    assert.equal(readFileSync(path, 'utf8'), existing);
  });
  check('invalid input fails without overwriting bytes', () => {
    const path = join(temp, 'invalid.json');
    for (const text of ['{broken', '{"version":3}', '{"version":1}', '{"version":1,"theme":null}']) {
      writeFileSync(path, text);
      const result = migrate(path);
      assert.ok(Number.isInteger(result.status) && result.status !== 0, 'must exit nonzero, not hang');
      assert.equal(readFileSync(path, 'utf8'), text);
    }
  });
} finally { rmSync(temp, { recursive: true, force: true }); }
console.log(JSON.stringify({ pass: checks.every(c => c.ok), checks }));
