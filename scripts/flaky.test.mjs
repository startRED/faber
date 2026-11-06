import test from 'node:test';
import assert from 'node:assert/strict';
import { CLEAN_NIGHTS, GONE_AFTER_DAYS, empty, parse, step, summarize } from './flaky.mjs';

const night = (n) => `2026-11-${String(10 + n).padStart(2, '0')}`;
const listed = (name, entry = {}) => ({ checks: { [name]: { since: '2026-11-06', clean: 0, seen: '2026-11-06', ...entry } } });

test('a check that failed and then passed joins the list, and a real failure does not', () => {
  const { state, added } = step(empty(), [{ flaky: ['chance'], failed: [], clean: [] }], { nightly: false, today: '2026-11-06' });
  assert.deepEqual(added, ['chance']);
  assert.deepEqual(state.checks.chance, { since: '2026-11-06', clean: 0, seen: undefined });
  // The runner names only checks that passed the second try; one that failed twice is in no report and fails the gate.
  assert.deepEqual(step(empty(), [{ flaky: [], failed: ['real'], clean: [] }], { nightly: false, today: '2026-11-06' }).state, empty());
});

test('reports of several parts are put together, and flaky again keeps the date it joined', () => {
  const reports = [{ flaky: ['a'] }, { flaky: ['b'], clean: ['a'] }, undefined];
  const { state, added } = step(listed('a', { since: '2026-10-01', clean: 3 }), reports, { nightly: true, today: '2026-11-07' });
  assert.deepEqual(added, ['b']);
  assert.deepEqual(state.checks.a, { since: '2026-10-01', clean: 0, seen: '2026-11-07' }, 'flaky wins over clean, the nights start again');
});

test('only a nightly run counts clean nights, and a check returns to the gate after CLEAN_NIGHTS in a row', () => {
  const clean = [{ clean: ['chance'] }];
  assert.equal(step(listed('chance', { clean: 2 }), clean, { nightly: false, today: night(1) }).state.checks.chance.clean, 2);
  let state = listed('chance');
  for (let n = 1; n < CLEAN_NIGHTS; n++) {
    const result = step(state, clean, { nightly: true, today: night(n) });
    assert.equal(result.state.checks.chance.clean, n);
    assert.deepEqual(result.returned, []);
    state = result.state;
  }
  const last = step(state, clean, { nightly: true, today: night(CLEAN_NIGHTS) });
  assert.deepEqual(last.returned, ['chance']);
  assert.deepEqual(last.state, empty());
});

test('running the daily run again the same night counts one clean night, and a failure that night still resets', () => {
  const clean = [{ clean: ['chance'] }];
  const first = step(listed('chance'), clean, { nightly: true, today: night(1) });
  const again = step(first.state, clean, { nightly: true, today: night(1) });
  assert.equal(again.state.checks.chance.clean, 1);
  assert.equal(step(first.state, [{ failed: ['chance'] }], { nightly: true, today: night(1) }).state.checks.chance.clean, 0);
  assert.equal(step(first.state, clean, { nightly: true, today: night(2) }).state.checks.chance.clean, 2);
});

test('a night with no word on the check starts the count again', () => {
  const state = listed('chance', { clean: 4, seen: night(0) });
  const silent = step(state, [{}], { nightly: true, today: night(1) });
  assert.equal(silent.state.checks.chance.clean, 0);
  assert.deepEqual(step(silent.state, [{ clean: ['chance'] }], { nightly: true, today: night(2) }).returned, []);
});

test('a night or a run that fails the listed check starts the count again', () => {
  const state = listed('chance', { clean: 4 });
  assert.equal(step(state, [{ failed: ['chance'] }], { nightly: true, today: night(1) }).state.checks.chance.clean, 0);
  assert.equal(step(state, [{ failed: ['chance'] }], { nightly: false, today: night(1) }).state.checks.chance.clean, 0);
  assert.equal(step(state, [{ flaky: ['chance'] }], { nightly: true, today: night(1) }).state.checks.chance.clean, 0);
});

test('a listed check the nights no longer see leaves the list, but a short silence keeps it', () => {
  const state = listed('renamed', { seen: '2026-11-06' });
  const soon = step(state, [{}], { nightly: true, today: `2026-11-${6 + GONE_AFTER_DAYS}` });
  assert.deepEqual(Object.keys(soon.state.checks), ['renamed']);
  const later = step(state, [{}], { nightly: true, today: `2026-11-${7 + GONE_AFTER_DAYS}` });
  assert.deepEqual(later.gone, ['renamed']);
  assert.deepEqual(later.state, empty());
  assert.deepEqual(step(state, [{}], { nightly: false, today: '2027-01-01' }).state, state, 'a push never forgets a check');
});

test('the state is never mutated, and a broken file reads as an empty list', () => {
  const state = listed('a');
  const copy = structuredClone(state);
  step(state, [{ flaky: ['b'], clean: ['a'] }], { nightly: true, today: night(1) });
  assert.deepEqual(state, copy);
  for (const text of ['', 'nope', '{}', '{"checks": []}', '{"checks": null}']) assert.deepEqual(parse(text), empty(), text);
  assert.deepEqual(parse('{"checks": {"a": {"since": "x"}}}').checks, { a: { since: 'x' } });
});

test('the morning words name each check, its nights and what changed', () => {
  assert.equal(summarize({ state: empty() }), 'No check fails by chance.');
  const text = summarize({ state: listed('slow frame', { clean: 2 }), added: ['slow frame'], returned: ['old'], gone: ['renamed'] });
  assert.match(text, /\(1\)/);
  assert.match(text, /- slow frame \(since 2026-11-06; 2 of 5 clean nights\)/);
  assert.match(text, /Joined the list: slow frame/);
  assert.match(text, /Back on the gate after 5 clean nights: old/);
  assert.match(text, /Left the list, no longer seen: renamed/);
});
