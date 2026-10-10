import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { insideFolder, newFiles, saveFiles } from '../src/engine/workspace.ts';

test('attachments get safe, unique names and are not reported as created by the answer', () => {
  const dir = mkdtempSync(join(tmpdir(), 'faber-ws-'));
  const since = Date.now();
  const body = Buffer.from('%PDF-1.4').toString('base64');
  const saved = saveFiles(join(dir, 'attachments'), [
    { name: 'report.pdf', base64: body },
    { name: 'report.pdf', base64: body },
    { name: '..\\..\\evil:name?.txt', base64: body },
  ]);
  assert.deepEqual(saved.map((p) => p.slice(dir.length + 1).replace(/\\/g, '/')), ['attachments/report.pdf', 'attachments/report-1.pdf', 'attachments/evil_name_.txt']);
  assert.deepEqual(newFiles(dir, since), []);
});

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
