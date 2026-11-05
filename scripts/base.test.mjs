// node --test scripts: a red phase-1-base is told with what failed, and only a push to it is refused.

import assert from 'node:assert/strict';
import test from 'node:test';
import { failures, message, pushedToBase, scopedTipPassed, tipPassed, verdict } from './base.mjs';

const run = (conclusion, n, status = 'completed') => ({
  status,
  conclusion,
  headSha: `${n}`.repeat(40),
  databaseId: n,
  createdAt: `2026-10-16T1${n}:14:00Z`,
  displayTitle: `commit ${n}`,
  url: `https://github.com/o/r/actions/runs/${n}`,
});

test('a green test-only successor clears red only with a complete ancestor and the exact tests run', () => {
  const red = verdict([run('failure', 1)]);
  const tip = '3'.repeat(40);
  const parent = '2'.repeat(40);
  const candidate = (n, event = 'workflow_dispatch') => ({ ...run('success', n), event, headBranch: 'queue/pr-2' });
  const good = (name) => ({ name, status: 'completed', conclusion: 'success', steps: [{ name: 'Run npm run lint', conclusion: 'success' }] });
  const full = ['changes', 'preflight', 'docs', 'app', 'costs', 'ui', 'gate', 'check (windows-latest, lints)',
    'check (windows-latest, rust)', 'check (macos-latest, lints rust ui)', ...Array.from({ length: 8 }, (_, i) => `screen (${i + 1}/8)`)].map(good);
  const scoped = ['changes', 'preflight', 'docs', 'gate', 'land', 'check (windows-latest, lints)', 'check (macos-latest, lints)'].map(good);
  const proof = {
    runsFor: (sha) => [candidate(sha === tip ? 3 : 2, sha === tip ? 'push' : 'workflow_dispatch')],
    jobsFor: (id) => id === 3 ? scoped : full,
    changeFor: () => ({ parents: [parent], changes: [{ path: 'scripts/cargo.test.mjs', oldMode: '100644', newMode: '100644', status: 'M' }] }),
    scriptsAt: () => ['scripts/cargo.test.mjs'],
    workflowAt: () => true,
  };
  assert.equal(scopedTipPassed(red, tip, proof), true);
  assert.equal(scopedTipPassed(red, tip, { ...proof, scriptsAt: () => [] }), false, 'an unregistered test is not proof');
  assert.equal(scopedTipPassed(red, tip, { ...proof, workflowAt: () => false }), false, 'the ancestor must run script tests');
  assert.equal(scopedTipPassed(red, tip, { ...proof, changeFor: () => ({ parents: [parent, '4'.repeat(40)], changes: [] }) }), false, 'merges do not qualify');
  for (const bad of [
    { path: 'ui/App.tsx', oldMode: '100644', newMode: '100644', status: 'M' },
    { path: 'scripts/cargo.test.mjs', oldMode: '100644', newMode: '100755', status: 'M' },
    { path: 'scripts/cargo.test.mjs', oldMode: '000000', newMode: '100644', status: 'A' },
  ]) assert.equal(scopedTipPassed(red, tip, { ...proof, changeFor: () => ({ parents: [parent], changes: [bad] }) }), false);
  const failed = { ...candidate(3, 'push'), conclusion: 'failure' };
  assert.equal(scopedTipPassed(red, tip, { ...proof, runsFor: (sha) => sha === tip ? [failed, candidate(3, 'push')] : [candidate(2)] }), false);
  const failedParent = { ...candidate(2), conclusion: 'failure' };
  assert.equal(scopedTipPassed(red, tip, { ...proof, runsFor: (sha) => sha === parent ? [failedParent, candidate(2)] : [candidate(3, 'push')] }), false,
    'a newer failed ancestor is not proof');
  assert.equal(scopedTipPassed(red, tip, { ...proof, jobsFor: (id) => id === 3 ? scoped.filter((job) => job.name !== 'land') : full }), false);
  assert.equal(scopedTipPassed(red, tip, { ...proof, jobsFor: (id) => id === 3 ? scoped.map((job) => job.name.includes('windows-latest') ? { ...job, steps: [] } : job) : full }), false);
  const middle = '4'.repeat(40);
  const reverted = { ...proof,
    runsFor: (sha) => sha === tip ? [candidate(3, 'push')] : sha === middle ? [] : [candidate(2)],
    changeFor: (sha) => sha === tip ? { parents: [middle], changes: [{ path: 'scripts/cargo.test.mjs', oldMode: '100644', newMode: '100644', status: 'M' }] }
      : { parents: [parent], changes: [{ path: 'ui/App.tsx', oldMode: '100644', newMode: '100644', status: 'M' }] },
  };
  assert.equal(scopedTipPassed(red, tip, reverted), false, 'an unrelated intermediate change cannot be hidden by a later revert');
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

test('only a complete, later queue check of the current tip clears an older red base', () => {
  const red = verdict([run('failure', 1)]);
  const tip = '2'.repeat(40);
  const candidate = { ...run('success', 2), event: 'workflow_dispatch', headBranch: 'queue/pr-2' };
  const names = [
    'changes', 'preflight', 'docs', 'app', 'costs', 'ui', 'gate',
    'check (windows-latest, lints)', 'check (windows-latest, rust)', 'check (macos-latest, lints rust ui)',
    ...Array.from({ length: 8 }, (_, i) => `screen (${i + 1}/8)`),
  ];
  const jobs = names.map((name) => ({ name, status: 'completed', conclusion: 'success' }));
  const passed = (sha, runs, list = jobs) => tipPassed(red, sha, runs, () => list);
  assert.equal(passed(tip, [candidate]), true);
  assert.equal(passed(tip, [{ ...candidate, event: 'push' }]), true, 'a complete direct queue push is also proof');
  assert.equal(passed(red.run.headSha, [candidate]), false, 'a failed current tip stays red');
  assert.equal(passed('3'.repeat(40), [candidate]), false, 'an older tip proves nothing about a newer one');
  assert.equal(passed(tip, [{ ...candidate, createdAt: red.run.createdAt }]), false);
  assert.equal(passed(tip, [{ ...candidate, headBranch: 'main' }]), false);
  assert.equal(passed(tip, [{ ...candidate, event: 'pull_request' }]), false);
  assert.equal(passed(tip, [candidate], jobs.filter((job) => job.name !== 'screen (4/8)')), false);
  assert.equal(passed(tip, [candidate], jobs.map((job) => job.name === 'ui' ? { ...job, conclusion: 'skipped' } : job)), false);
  assert.equal(passed(tip, [{ ...candidate, conclusion: 'failure' }, candidate]), false, 'a newer failed check wins');
  assert.equal(tipPassed(red, tip, [{ ...candidate, databaseId: 3, conclusion: 'failure' }, candidate], (id) => id === 3 ? [] : jobs), false,
    'a newer failure without a gate also wins');
  assert.equal(tipPassed(red, tip, [{ ...candidate, databaseId: 3, conclusion: 'cancelled' }, candidate], () => jobs), true,
    'a cancelled run says nothing');
  assert.equal(passed(tip, [candidate], [{ name: 'train', status: 'completed', conclusion: 'success' }]), false, 'a waiting train is not a gate');
  assert.equal(tipPassed(red, tip, [{ ...candidate, databaseId: 3 }, candidate], (id) => id === 3 ? [] : jobs), true);
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
