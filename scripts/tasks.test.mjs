// node --test scripts: the task queue gives tasks in order, one session each, and takes a stale claim over.

import assert from 'node:assert/strict';
import test from 'node:test';
import { claimMessage, finished, parseName, parseTask, queue, readClaim, STALE_MS } from './tasks.mjs';

test('a task file is <order>-<name>.md; anything else in the folder is not a task', () => {
  assert.deepEqual(parseName('010-light-proofs.md'), { order: 10, name: 'light-proofs' });
  assert.equal(parseName('README.md'), null);
  assert.equal(parseName('draft.md'), null);
  assert.equal(parseName('020-Upper.md'), null);
});

test('the title is the first heading and the brief is what follows it', () => {
  assert.deepEqual(parseTask('# Do X\r\n\r\nThe files are a and b.\r\nDone when c.\r\n'), { title: 'Do X', brief: 'The files are a and b.\nDone when c.' });
  assert.deepEqual(parseTask('No heading.'), { title: '', brief: 'No heading.' });
});

test('a claim says who holds the task and since when', () => {
  const at = new Date('2026-10-17T12:00:00Z');
  assert.deepEqual(readClaim(claimMessage('x', 'session 7', at)), { who: 'session 7', since: at.getTime() });
  assert.deepEqual(readClaim('claim x\n'), { who: '', since: 0 });
});

test('tasks come lowest order first; a claimed one is skipped until its claim is a day old', () => {
  const now = Date.parse('2026-10-17T12:00:00Z');
  const claims = new Map([
    ['b', { who: 's1', since: now - 60_000 }],
    ['c', { who: 's2', since: now - STALE_MS - 1 }],
  ]);
  const order = queue(['README.md', '030-c.md', '010-b.md', '020-a.md', '020-0.md'], claims, now);
  assert.deepEqual(order.map((t) => [t.name, t.free]), [['b', false], ['0', true], ['a', true], ['c', true]]);
});

test('a claim whose task left the list is finished', () => {
  const claims = new Map([['gone', {}], ['kept', {}]]);
  assert.deepEqual(finished(['010-kept.md', 'README.md'], claims), ['gone']);
});
