// node --test scripts: the task queue gives tasks in order, one session each, and takes a stale claim over.

import assert from 'node:assert/strict';
import test from 'node:test';
import { ago, board, BOARD_ROWS, claimMessage, finished, parseDepends, parseName, parseTask, queue, readClaim, repoUrl, STALE_MS } from './tasks.mjs';

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

test('a task waits while a task it depends on is still on the list', () => {
  assert.deepEqual(parseDepends('# T\n\nArea: none\nDepends: a-one, b-two\n'), ['a-one', 'b-two']);
  assert.deepEqual(parseDepends('# T\n\nDepends: none\n'), []);
  assert.deepEqual(parseDepends('# T\n\nNo line.\n'), []);
  const depends = new Map([['c', ['a', 'gone']], ['b', ['gone']]]);
  const [a, b, c] = queue(['010-a.md', '020-b.md', '030-c.md'], new Map(), 0, depends);
  assert.deepEqual([a.free, b.free, c.free], [true, true, false]);
  assert.deepEqual(c.waits, ['a']);
});

test('the board sorts every task into doing, next or waiting, and links landed pull requests', () => {
  const now = Date.parse('2026-10-30T12:00:00Z');
  const claims = new Map([
    ['held', { who: 'team-3', since: now - 3 * 3600_000 }],
    ['old', { who: 'team-5', since: now - STALE_MS - 1 }],
  ]);
  const depends = new Map([['later', ['held']]]);
  const tasks = queue(['010-held.md', '020-old.md', '030-free.md', '040-later.md'], claims, now, depends).map((t) => ({ ...t, title: `Title ${t.name}`, area: t.name === 'free' ? 'Composer' : '' }));
  const text = board({
    tasks,
    landing: [{ subject: 'fix: queued (#7)', since: now - 20 * 60_000 }],
    done: [{ date: '2026-10-30', subject: 'feat: landed (#5)' }],
    now,
    repo: 'https://github.com/o/r',
  });
  const section = (name) => text.split(`## ${name}\n\n`)[1].split('\n\n')[0].trimEnd();
  assert.match(text, /\*\*1 doing · 1 landing · 2 next · 1 waiting\*\*/);
  assert.equal(section('Doing'), '- **Title held** (`held`), team-3, for 3 h');
  assert.equal(section('Landing'), '- fix: queued ([#7](https://github.com/o/r/pull/7)), queued 20 min ago');
  assert.equal(section('Next'), '- **Title old** (`old`), claim by team-5 gone stale\n- **Title free** (`free`), Composer');
  assert.equal(section('Waiting on other tasks'), '- **Title later** (`later`) waits on `held`');
  assert.equal(section('Landed'), '- 2026-10-30: feat: landed ([#5](https://github.com/o/r/pull/5))');
});

test('the board keeps its size whatever the length of the queue', () => {
  const many = Array.from({ length: 200 }, (_, i) => `${String(i).padStart(3, '0')}-t${i}.md`);
  const tasks = queue(many, new Map(), 0).map((t) => ({ ...t, title: 'x'.repeat(100), area: '' }));
  const text = board({ tasks, landing: [], done: [], now: 0 });
  assert.match(text, new RegExp(`- and ${200 - BOARD_ROWS} more`));
  assert.ok(text.split('\n').length < 40 && Buffer.byteLength(text) < 3000);
  assert.match(text, /Nobody holds a task\.[\s\S]*Nothing is being checked to land\.[\s\S]*Nothing yet\./);
});

test('durations and the repository link read as a person reads them', () => {
  assert.deepEqual([ago(59_000), ago(90 * 60_000), ago(47 * 3600_000), ago(72 * 3600_000), ago(-5)], ['1 min', '2 h', '47 h', '3 d', '0 min']);
  assert.equal(repoUrl('https://github.com/startRED/faber.git\n'), 'https://github.com/startRED/faber');
  assert.equal(repoUrl('git@github.com:startRED/faber.git'), 'https://github.com/startRED/faber');
  assert.equal(repoUrl('https://gitlab.com/a/b.git'), '');
});
