import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('../../bench/proofs/organize.mjs', import.meta.url));

function proof(change = () => {}) {
  const root = mkdtempSync(join(tmpdir(), 'faber-organize-proof-'));
  try {
    const pristine = join(root, 'before');
    const after = join(root, 'after');
    mkdirSync(pristine);
    const groups = {
      Notes: { 'meeting.txt': 'Meeting notes\n', 'meeting-backup.txt': 'Meeting notes\n' },
      Finance: { 'invoice.txt': 'Invoice 42\n', 'budget.txt': 'Budget 2026\n' },
      Photos: { 'beach.jpg': Buffer.from([255, 216, 255, 0]), 'garden.jpg': Buffer.from([255, 216, 255, 1]) },
    };
    for (const [group, files] of Object.entries(groups)) {
      mkdirSync(join(after, group), { recursive: true });
      for (const [name, content] of Object.entries(files)) {
        writeFileSync(join(pristine, name), content);
        writeFileSync(join(after, group, name), content);
      }
    }
    writeFileSync(join(after, 'INDEX.md'), '# Files\n\nNotes: meeting notes. Finance: invoices and budgets. Photos: pictures.\n');
    change(after);
    return JSON.parse(execFileSync(process.execPath, [script, after, pristine], { encoding: 'utf8', windowsHide: true, timeout: 5000 }));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test('organizing keeps both original files when their contents match', () => {
  assert.equal(proof().pass, true);
});

test('organizing cannot delete one of two original files with matching contents', () => {
  const result = proof((after) => rmSync(join(after, 'Notes', 'meeting-backup.txt')));
  assert.equal(result.pass, false);
  assert.equal(result.checks.find((c) => c.name === 'every original file is still there, content unchanged').ok, false);
});

test('organizing cannot add another copy of an original file', () => {
  const result = proof((after) => cpSync(join(after, 'Notes', 'meeting.txt'), join(after, 'Notes', 'meeting-extra.txt')));
  assert.equal(result.pass, false);
  assert.equal(result.checks.find((c) => c.name === 'no file was copied instead of moved').ok, false);
});

test('organizing still rejects changed binary content', () => {
  const result = proof((after) => writeFileSync(join(after, 'Photos', 'beach.jpg'), Buffer.from([255, 216, 255, 9])));
  assert.equal(result.pass, false);
  assert.equal(result.checks.find((c) => c.name === 'every original file is still there, content unchanged').ok, false);
});

test('the real organizing fixture preserves its existing backup and detects its loss', () => {
  const root = mkdtempSync(join(tmpdir(), 'faber-organize-fixture-'));
  try {
    const pristine = join(root, 'before');
    const after = join(root, 'after');
    const fixture = fileURLToPath(new URL('../../bench/fixtures/organize.mjs', import.meta.url));
    execFileSync(process.execPath, [fixture, pristine], { windowsHide: true, timeout: 5000 });
    cpSync(pristine, after, { recursive: true });
    for (const name of readdirSync(after)) {
      const group = /\.(jpg|png)$/.test(name) ? 'Photos' : /\.(sh|py|js)$/.test(name) ? 'Scripts' : /\.zip$/.test(name) ? 'Backups' : 'Documents';
      mkdirSync(join(after, group), { recursive: true });
      renameSync(join(after, name), join(after, group, name));
    }
    writeFileSync(join(after, 'INDEX.md'), 'Photos: pictures. Scripts: utilities. Backups: archives. Documents: notes and records.\n');
    const result = () => JSON.parse(execFileSync(process.execPath, [script, after, pristine], { encoding: 'utf8', windowsHide: true, timeout: 5000 }));
    assert.equal(result().pass, true, 'all original fixture files, including the existing backup, survive');
    rmSync(join(after, 'Documents', 'meeting notes oct backup.txt'));
    assert.equal(result().pass, false, 'losing the fixture backup must fail even while its matching original remains');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
