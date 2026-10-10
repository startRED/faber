import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { insideFolder, newFiles } from '../src/engine/workspace.ts';

test('only files changed since the turn started are listed, with forward slashes', () => {
  const dir = mkdtempSync(join(tmpdir(), 'faber-ws-'));
  writeFileSync(join(dir, 'old.txt'), 'old');
  const past = new Date(Date.now() - 60_000);
  utimesSync(join(dir, 'old.txt'), past, past);
  const since = Date.now();
  mkdirSync(join(dir, 'out'));
  writeFileSync(join(dir, 'out', 'chart.png'), 'png');
  assert.deepEqual(newFiles(dir, since), [{ path: 'out/chart.png', size: 3 }]);
});

test('paths outside the workspace are refused', () => {
  const dir = mkdtempSync(join(tmpdir(), 'faber-ws-'));
  assert.equal(insideFolder(dir, 'a.txt'), join(dir, 'a.txt'));
  assert.equal(insideFolder(dir, '../secret.txt'), undefined);
  assert.equal(insideFolder(dir, join(tmpdir(), 'elsewhere.txt')), undefined);
  assert.equal(insideFolder(dir, '.'), undefined);
});
