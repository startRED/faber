// node --test scripts: a lane that queued a pull request fixes it before new work, and pushes only its fix meanwhile.

import assert from 'node:assert/strict';
import test from 'node:test';
import { message, onlyOwed, owed, record } from './owed.mjs';

const status = (state) => [{ __typename: 'StatusContext', context: 'queue', state }, { __typename: 'CheckRun', name: 'gate', conclusion: 'SUCCESS' }];

test('a pull request queued again moves to the end of the record once', () => {
  const list = record(record(record([], 352, 'a'), 353, 'b'), 352, 'a2');
  assert.deepEqual(list, [{ number: 353, branch: 'b' }, { number: 352, branch: 'a2' }]);
});

test('a red queue run or a push after queueing is owed; a queued, landed or closed one is not', () => {
  const list = [352, 353, 354, 355].map((number) => ({ number, branch: `b${number}` }));
  const open = [
    { number: 352, headRefName: 'b352', statusCheckRollup: status('FAILURE') },
    { number: 353, headRefName: 'b353', statusCheckRollup: [{ __typename: 'CheckRun', name: 'gate', conclusion: 'FAILURE' }] },
    { number: 354, headRefName: 'b354', statusCheckRollup: status('PENDING') },
    { number: 999, headRefName: 'other', statusCheckRollup: status('FAILURE') },
  ];
  const { owed: found, kept } = owed(list, open);
  assert.deepEqual(found, [
    { number: 352, branch: 'b352', why: 'red' },
    { number: 353, branch: 'b353', why: 'unqueued' },
  ]);
  assert.deepEqual(kept.map((entry) => entry.number), [352, 353, 354], 'the landed or closed #355 is forgotten');
  assert.deepEqual(owed(list, open, true).kept, list, 'a list cut at its limit forgets nothing');
  assert.match(message(found), /#352 b352: its queue run failed\n {2}#353 b353: it changed after it was queued/);
});

test('while one is owed, only its own branches are pushed, and deleting a branch is always allowed', () => {
  const found = [{ number: 352, branch: 'chat-workspace-recovery', why: 'red' }];
  const line = (ref, sha = 'a'.repeat(40)) => `refs/heads/x ${sha} ${ref} ${'0'.repeat(40)}`;
  assert.ok(onlyOwed(line('refs/heads/chat-workspace-recovery'), found));
  assert.ok(onlyOwed([line('refs/heads/queue/pr-352'), line('refs/heads/verify/pr-352')].join('\n'), found));
  assert.ok(onlyOwed(line('refs/heads/claim/some-task', '0'.repeat(40)), found));
  assert.ok(onlyOwed('', found));
  assert.ok(!onlyOwed(line('refs/heads/project-start-card-preparation-cost'), found));
  assert.ok(!onlyOwed([line('refs/heads/chat-workspace-recovery'), line('refs/heads/queue/new-work')].join('\n'), found));
});
