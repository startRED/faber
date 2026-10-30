import { test } from 'node:test';
import assert from 'node:assert/strict';
import { median, p90, percentile, inWindow, summarize, format, countFree } from './pace.mjs';

test('percentiles use the nearest rank', () => {
  assert.equal(median([]), null);
  assert.equal(median([5, 1, 3]), 3);
  assert.equal(median([1, 2, 3, 4]), 2);
  assert.equal(p90([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]), 9);
  assert.equal(p90([7]), 7);
  assert.equal(percentile([1, 2], 100), 2);
});

const now = Date.parse('2026-10-30T12:00:00Z');
const pr = (mergedAt, createdAt, additions, deletions) => ({ mergedAt, createdAt, additions, deletions });

test('a window keeps only what happened inside it', () => {
  const items = [{ t: '2026-10-30T11:00:00Z' }, { t: '2026-10-29T11:00:00Z' }, { t: '2026-10-30T13:00:00Z' }];
  assert.equal(inWindow(items, 't', now, 24 * 3600e3).length, 1);
  assert.equal(inWindow(items, 't', now, 48 * 3600e3).length, 2);
});

test('the summary counts pull requests, lines, minutes and rounds', () => {
  const prs = [
    pr('2026-10-30T11:00:00Z', '2026-10-30T10:30:00Z', 100, 20),
    pr('2026-10-30T09:00:00Z', '2026-10-30T08:00:00Z', 200, 100),
    pr('2026-10-27T09:00:00Z', '2026-10-27T08:50:00Z', 5, 5),
  ];
  const runs = [
    { createdAt: '2026-10-30T10:00:00Z', conclusion: 'success' },
    { createdAt: '2026-10-30T10:05:00Z', conclusion: 'failure' },
    { createdAt: '2026-10-30T10:06:00Z', conclusion: 'cancelled' },
    { createdAt: '2026-10-25T10:00:00Z', conclusion: 'success' },
  ];
  const day = summarize(prs, runs, now, 24 * 3600e3);
  assert.deepEqual(day, { landed: 2, lines: { median: 120, p90: 300 }, minutes: { median: 30, p90: 60 }, rounds: { green: 1, red: 1, cancelled: 1 } });
  assert.equal(summarize(prs, runs, now, 7 * 24 * 3600e3).landed, 3);
  assert.equal(summarize(prs, runs, now, 7 * 24 * 3600e3).rounds.green, 2);
});

test('an empty window prints dashes, not NaN', () => {
  const text = format('24 h', summarize([], [], now, 3600e3), 4);
  assert.match(text, /0 pull requests landed/);
  assert.match(text, /median -, p90 -/);
  assert.match(text, /free tasks now: 4/);
});

test('free tasks are the listed ones without a live claim', () => {
  assert.equal(countFree('010 a  free\n020 b  claimed by x\n030 c  stale claim by y\n040 d  waits on a\n'), 2);
  assert.equal(countFree(''), 0);
});
