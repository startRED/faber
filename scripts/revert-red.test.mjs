import test from 'node:test';
import assert from 'node:assert/strict';
import { decide, notice, pullRequest, revertMessage, revertsOf } from './revert-red.mjs';

test('only a red commit on a green parent gate, not fixed later, is reverted', () => {
  const input = { subject: 'fix: a', parentGate: 'success', reverted: false, fixedLater: false, runnerUnavailable: false };
  assert.equal(decide(input), 'revert');
  for (const parentGate of [null, 'cancelled', 'failure']) assert.equal(decide({ ...input, parentGate }), 'wait');
  assert.equal(decide({ ...input, reverted: true }), 'stale');
  assert.equal(decide({ ...input, fixedLater: true }), 'stale');
  assert.equal(decide({ ...input, runnerUnavailable: true }), 'skip');
  assert.equal(decide({ ...input, subject: 'Revert previous' }), 'skip');
  assert.equal(decide({ ...input, subject: 'revert: previous' }), 'skip');
});
test('a revert is found by its trailer or by git revert\'s own line', () => {
  assert.equal(revertsOf('revert: x\n\nReverts: abc\n\0other\0', 'abc'), true);
  assert.equal(revertsOf('Revert "x"\n\nThis reverts commit abc.\n\0', 'abc'), true);
  assert.equal(revertsOf('fix: y\n\nReverts: abcd\n\0', 'abc'), false);
});
test('messages preserve the run, commit and optional pull request', () => {
  assert.equal(pullRequest('fix: a\n\nPull-request: #42\n'), '42');
  assert.equal(pullRequest('fix: a'), undefined);
  assert.equal(revertMessage('fix: a', 'abc', 'url'), 'revert: fix: a\n\nFailed run: url\n\nReverts: abc\n');
  assert.match(notice('abc', 'url'), /abc was reverted/);
  assert.match(notice('abc', 'url'), /node scripts\/ship\.mjs/);
});
