// node --test scripts: the space report says what would go and why the rest stays, and never says "would go" by guess.

import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { CAP, cacheVerdict, folderSize, OLD, overCap, parseWorktrees, readCaches, readWorktree, totals, worktreeVerdict } from './space.mjs';

const DAY = 24 * 60 * 60 * 1000;
const NOW = Date.parse('2026-11-26T12:00:00Z');

test('worktree records come from git porcelain, the main checkout first', () => {
  const records = parseWorktrees(
    'worktree F:/faber\nHEAD aaa\nbranch refs/heads/main\n\nworktree F:/lane\nHEAD bbb\ndetached\n\nworktree F:/gone\nHEAD ccc\nbranch refs/heads/old\nprunable gitdir file points to non-existent location\n',
  );
  assert.deepEqual(records.map((r) => [r.path, r.branch, r.detached, r.prunable]), [
    ['F:/faber', 'main', false, false],
    ['F:/lane', null, true, false],
    ['F:/gone', 'old', false, true],
  ]);
});

/** A fake git for one worktree: a merged branch, a clean or dirty status, a commit at `committed`. */
function fakeGit({ merged = true, status = '', committed = NOW - 30 * DAY, fails = false }) {
  return (args) => {
    if (fails) throw new Error('fatal: not a git repository');
    if (args[0] === 'status') return status;
    if (args[0] === 'merge-base' && args[1] === '--is-ancestor') {
      if (merged) return '';
      throw new Error('exit 1');
    }
    if (args[0] === 'merge-base') return 'fork\n';
    if (args[0] === 'diff' && args[1] === '--name-only') return 'a.rs\n';
    if (args[0] === 'diff' && args[1] === '--quiet') throw new Error('exit 1');
    if (args[0] === 'log') return `${Math.floor(committed / 1000)}\n`;
    throw new Error(`unexpected git ${args.join(' ')}`);
  };
}

test('a merged, clean, old worktree would go; a dirty one stays; an unreadable one is unknown', () => {
  const temp = mkdtempSync(path.join(os.tmpdir(), 'faber-space-'));
  try {
    const record = { path: temp, head: 'bbb', branch: 'lane', prunable: false, locked: false };
    // The folder was just made, so "a month from now" makes it and its commit a month old.
    const merged = readWorktree(record, fakeGit({}), Date.now() + 30 * DAY);
    assert.equal(worktreeVerdict(merged).verdict, 'would go', JSON.stringify(merged));
    const dirty = readWorktree(record, fakeGit({ status: ' M a.rs\n' }), Date.now() + 30 * DAY);
    assert.deepEqual(worktreeVerdict(dirty), { verdict: 'keep', why: 'uncommitted or untracked changes' });
    const unread = readWorktree(record, fakeGit({ fails: true }), Date.now() + 30 * DAY);
    assert.equal(worktreeVerdict(unread).verdict, 'unknown');
    const ahead = readWorktree(record, fakeGit({ merged: false }), Date.now() + 30 * DAY);
    assert.deepEqual(worktreeVerdict(ahead), { verdict: 'keep', why: 'work not on origin/phase-1-base' });
    const recent = readWorktree(record, fakeGit({}), Date.now() + 1000);
    assert.equal(worktreeVerdict(recent).verdict, 'keep', 'a worktree touched this week stays');
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test('the main checkout, a locked worktree and an unread age never go; a missing folder is only a record', () => {
  assert.equal(worktreeVerdict({ main: true, merged: true, clean: true, age: OLD * 2 }).verdict, 'keep');
  assert.equal(worktreeVerdict({ locked: true, merged: true, clean: true, age: OLD * 2 }).verdict, 'keep');
  assert.equal(worktreeVerdict({ merged: true, clean: true, age: null }).verdict, 'unknown');
  assert.equal(worktreeVerdict({ prunable: true }).verdict, 'would go');
});

test('a build folder in use, leased or unread stays; one idle past two hours would go', () => {
  const at = (hours) => NOW - hours * 60 * 60 * 1000;
  assert.equal(cacheVerdict({ used: at(0.2), now: NOW }).verdict, 'keep');
  assert.equal(cacheVerdict({ used: at(1), now: NOW }).verdict, 'keep');
  assert.equal(cacheVerdict({ used: at(5), now: NOW }).verdict, 'would go');
  assert.equal(cacheVerdict({ used: at(50), leased: true, now: NOW }).verdict, 'keep');
  assert.equal(cacheVerdict({ used: at(50), mainOwn: true, now: NOW }).verdict, 'keep');
  assert.equal(cacheVerdict({ used: null, now: NOW }).verdict, 'unknown');
});

test('sizes skip links, totals go by verdict, and the report says which protection passed the cap', () => {
  const temp = mkdtempSync(path.join(os.tmpdir(), 'faber-space-'));
  try {
    mkdirSync(path.join(temp, 'a'));
    writeFileSync(path.join(temp, 'a', 'one'), 'x'.repeat(100));
    const outside = mkdtempSync(path.join(os.tmpdir(), 'faber-space-out-'));
    writeFileSync(path.join(outside, 'big'), 'y'.repeat(5000));
    try {
      symlinkSync(outside, path.join(temp, 'link'), 'junction');
    } catch {
      // Making a link may need rights the test lacks; the size is then plain.
    }
    assert.equal(folderSize(temp), 100, 'a linked folder is not counted');
    rmSync(outside, { recursive: true, force: true });
    assert.equal(folderSize(path.join(temp, 'missing')), null, 'an unreadable folder has no size, not 0');
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
  const caches = [
    { size: CAP, verdict: 'keep', why: 'leased by a build right now' },
    { size: 2 * 1024 ** 3, verdict: 'would go', why: 'idle 9 hours' },
    { size: null, verdict: 'unknown', why: 'could not be read' },
  ];
  const t = totals(caches);
  assert.equal(t['would go'].bytes, 2 * 1024 ** 3);
  assert.equal(t.unknownSizes, 1);
  assert.match(overCap(caches), /over the 24\.0 GB budget, because the sweep keeps 24\.0 GB leased by a build right now; 2\.0 GB would go/);
});

test('the leased build slots are listed though Cargo never tagged them, and an untagged other folder is not', () => {
  const main = mkdtempSync(path.join(os.tmpdir(), 'faber-space-main-'));
  try {
    for (const name of ['build-1', 'tmp', 'debug']) mkdirSync(path.join(main, 'target', name, 'debug', 'deps'), { recursive: true });
    writeFileSync(path.join(main, 'target', 'build-1', 'debug', 'deps', 'lib.rlib'), 'z'.repeat(300));
    const caches = readCaches(main, [{ path: main }], Date.now());
    assert.deepEqual(caches.map((c) => path.basename(c.dir)).sort(), ['build-1', 'debug']);
    assert.equal(caches.find((c) => c.dir.endsWith('build-1')).size, 300);
  } finally {
    rmSync(main, { recursive: true, force: true });
  }
});
