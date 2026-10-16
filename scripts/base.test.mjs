// node --test scripts: a red phase-1-base is told with what failed, and only a push to it is refused.

import assert from 'node:assert/strict';
import test from 'node:test';
import { failures, message, pushedToBase, verdict } from './base.mjs';

const run = (conclusion, n, status = 'completed') => ({
  status,
  conclusion,
  headSha: `${n}`.repeat(40),
  databaseId: n,
  createdAt: `2026-10-16T1${n}:14:00Z`,
  displayTitle: `commit ${n}`,
  url: `https://github.com/o/r/actions/runs/${n}`,
});

test('a base whose newest finished run passed, or that has none, is not red', () => {
  assert.equal(verdict([run('success', 5), run('failure', 4)]), null);
  assert.equal(verdict([run('cancelled', 5), run('success', 4), run('failure', 3)]), null, 'a cancelled run says nothing');
  assert.equal(verdict([]), null);
  assert.equal(verdict([run('', 5, 'in_progress')]), null);
});

test('a red base says since when, and which runs still going may be the fix', () => {
  const red = verdict([run('', 9, 'in_progress'), run('failure', 8), run('cancelled', 7), run('failure', 6), run('success', 5)]);
  assert.equal(red.run.databaseId, 8);
  assert.equal(red.since.databaseId, 6);
  assert.deepEqual(red.newer.map((r) => r.databaseId), [9]);
  const text = message(red, ['check (macos-latest, lints rust ui): FAIL ui projects home: text sits on the veil']);
  assert.match(text, /^phase-1-base is red since 16:14 UTC: commit 8 \(8888888\)/);
  assert.match(text, /projects home: text sits on the veil/);
  assert.match(text, /still running: commit 9/);
});

test('the failing checks are read from the failed log, each once, with their job', () => {
  const job = 'check (macos-latest, lints rust ui)\tRun cargo t --suite ui\t';
  const log = [
    `${job}2026-10-16T17:49:28.6170920Z   CACHE_ON_FAILURE: false`,
    `${job}2026-10-16T17:54:18.5507060Z FAIL  ui      projects home: text sits only on the faint veil (645 ms)`,
    `${job}2026-10-16T17:54:18.5507060Z FAIL  ui      projects home: text sits only on the faint veil (645 ms)`,
    `${job}2026-10-16T17:54:18.5519530Z 243 passed, 1 failed, 0 not run, 0 skipped.`,
    `check (windows-latest, lints rust ui)\tRun cargo t --suite rust\t2026-10-16T17:50:00.0Z engine/src/a.rs:3:5: error[E0425]: cannot find value`,
  ].join('\n');
  assert.deepEqual(failures(log), [
    'check (macos-latest, lints rust ui): FAIL ui projects home: text sits only on the faint veil',
    'check (windows-latest, lints rust ui): engine/src/a.rs:3:5: error[E0425]: cannot find value',
  ]);
});

test('only a push that updates phase-1-base counts, never a deletion or another branch', () => {
  const a = 'a'.repeat(40);
  const zero = '0'.repeat(40);
  assert.deepEqual(pushedToBase(`HEAD ${a} refs/heads/phase-1-base ${zero}\n`), [a]);
  assert.deepEqual(pushedToBase(`refs/heads/x ${a} refs/heads/lane ${zero}\n`), []);
  assert.deepEqual(pushedToBase(`(delete) ${zero} refs/heads/phase-1-base ${a}\n`), []);
  assert.deepEqual(pushedToBase(''), []);
});
