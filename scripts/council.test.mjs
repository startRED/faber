import assert from 'node:assert/strict';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { aggregate, climb, firstJson, insideRepo, judgePrompt, LINES, median, parseVerdict, report, scorePhoto } from './council.mjs';

const P = 'critique-chat-home-1920x1080.png';
const Q = 'critique-settings-1280x800.png';
const full = [5, 5, 5, 5, 5, 5, 5, 5];
const answer = (photos) => JSON.stringify({ photos });
const judged = (lines, rows = []) => ({ lines, rows });
const row = (line, what = 'x') => ({ line, what, should: 'y' });

test('the prompt names every photo and every line, and the references only when there are some', () => {
  const plain = judgePrompt([P, Q]);
  for (const name of [P, Q, ...LINES]) assert.ok(plain.includes(name), name);
  assert.ok(!plain.includes('Reference photos'));
  assert.ok(judgePrompt([P], ['ref-other.png']).includes('ref-other.png'));
});

test('the first JSON object is found inside fences and words, with braces in its strings', () => {
  assert.deepEqual(firstJson('Here:\n```json\n{"a":"{x}","b":{"c":1}}\n```\nDone {not}'), { a: '{x}', b: { c: 1 } });
  assert.equal(firstJson('no json'), null);
  assert.equal(firstJson('{broken'), null);
});

test('a fair answer scores every photo on eight lines of 0 to 5 with a row per lost line', () => {
  const ok = parseVerdict(answer([{ photo: P, lines: [5, 5, 4, 5, 5, 5, 5, 5], rows: [row(3)] }]), [P]);
  assert.equal(ok.error, undefined);
  assert.deepEqual(ok.photos[P].lines, [5, 5, 4, 5, 5, 5, 5, 5]);
});

test('an answer that skips a photo, breaks the scale or loses a point with no row is no answer', () => {
  assert.match(parseVerdict(answer([{ photo: P, lines: full, rows: [] }]), [P, Q]).error, /settings/);
  assert.match(parseVerdict(answer([{ photo: P, lines: [6, 5, 5, 5, 5, 5, 5, 5] }]), [P]).error, /fair/);
  assert.match(parseVerdict(answer([{ photo: P, lines: [5, 5, 5] }]), [P]).error, /fair/);
  assert.match(parseVerdict(answer([{ photo: P, lines: [5, 5, 5, 5, 3, 5, 5, 5], rows: [row(2)] }]), [P]).error, /line 5/);
  assert.match(parseVerdict('I think it looks great!', [P]).error, /JSON/);
});

test('the median takes the lower middle when the count is even', () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([4, 2]), 2);
});

test('a judge far from the others is the jester: its numbers and rows leave the score', () => {
  const s = scorePhoto({
    codex: judged([5, 5, 4, 5, 5, 5, 5, 5], [row(3, 'two weights')]),
    claude: judged([5, 5, 4, 5, 5, 5, 4, 5], [row(3, 'two weights'), row(7, 'busy')]),
    agy: judged([1, 1, 1, 1, 1, 1, 1, 1], LINES.map((_, i) => row(i + 1, 'invented'))),
  });
  assert.deepEqual(s.jesters, ['agy']);
  assert.equal(s.total, 38);
  assert.deepEqual(s.agreed.map((r) => r.line), [3, 3]);
  assert.ok(s.unconfirmed.some((r) => r.judge === 'claude' && r.line === 7));
  assert.ok(s.unconfirmed.filter((r) => r.jester).length === 8);
});

test('two judges have no jester and the harsher one sets each line', () => {
  const s = scorePhoto({ codex: judged(full), claude: judged([1, 5, 5, 5, 5, 5, 5, 5], [row(1)]) });
  assert.deepEqual(s.jesters, []);
  assert.equal(s.total, 36);
  assert.equal(s.agreed.length, 0);
});

test('the council needs two fair answers and names the judges that gave none', () => {
  const v = { codex: parseVerdict(answer([{ photo: P, lines: full }]), [P]), agy: { error: 'timed out' } };
  const r = aggregate(v, [P]);
  assert.match(r.error, /two judges/);
  assert.deepEqual(r.failed, { agy: 'timed out' });
});

test('references get the median total and the report says the gap to the best', () => {
  const ref = 'ref-other.png';
  const fairOne = JSON.stringify({ photos: [{ photo: P, lines: full }], refs: [{ photo: ref, lines: [4, 4, 4, 4, 4, 4, 4, 4] }], better: ['quiet sidebar'] });
  const v = { codex: parseVerdict(fairOne, [P], [ref]), claude: parseVerdict(fairOne, [P], [ref]) };
  const r = aggregate(v, [P], [ref]);
  assert.equal(r.refTotals[ref], 32);
  const text = report('after', r, [P], null);
  assert.match(text, /Gap to the best: critique-chat-home-1920x1080\.png 8/);
  assert.match(text, /quiet sidebar/);
  assert.match(text, /Nothing the judges agree on/);
});

test('a climb keeps a round that rose with no drop, reverts a drop or a flat round, and stops at the target', () => {
  assert.equal(climb(null, { [P]: 30 }, 36).step, 'keep');
  assert.equal(climb({ [P]: 30, [Q]: 33 }, { [P]: 33, [Q]: 33 }, 36).step, 'keep');
  assert.equal(climb({ [P]: 30, [Q]: 33 }, { [P]: 35, [Q]: 30 }, 36).step, 'revert');
  assert.equal(climb({ [P]: 30 }, { [P]: 31 }, 36).step, 'revert', 'one point is noise');
  assert.equal(climb({ [P]: 30, [Q]: 33 }, { [P]: 36, [Q]: 37 }, 36).step, 'stop');
  assert.equal(climb({ [P]: 34, [Q]: 32 }, { [P]: 33, [Q]: 34 }, 36).step, 'keep', 'a one-point wobble is noise');
});

test('a climb never stops on a drop that still clears the target, nor keeps a round that lost a screen', () => {
  assert.equal(climb({ [P]: 40 }, { [P]: 36 }, 36).step, 'revert');
  const lost = climb({ [P]: 32, [Q]: 30 }, { [Q]: 32 }, 36);
  assert.equal(lost.step, 'revert');
  assert.match(lost.why, /missing/);
});

test('judges spread so wide that one would be left name no jester', () => {
  const spread = (n) => judged([n, n, n, n, 0, 0, 0, 0], [5, 6, 7, 8].map((l) => row(l)));
  const s = scorePhoto({ codex: spread(4), claude: spread(3), agy: spread(2) });
  assert.deepEqual(s.jesters, []);
  assert.equal(s.total, 12);
});

test('a judge never runs inside the repository', () => {
  const root = join(tmpdir(), 'nowhere', 'faber');
  assert.equal(insideRepo(join(tmpdir(), 'faber-council-x'), root), false);
  assert.equal(insideRepo(join(root, 'target', 'x'), root), true);
});
