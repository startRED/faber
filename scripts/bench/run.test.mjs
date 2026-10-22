import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MATCHED, median, medians, plan, table } from './run.mjs';

test('the medians leave the warm-up out and keep the order the runs came in', () => {
  const run = (label, wall_s, warmup = false) => ({ label, wall_s, procs_started: 1, cpu_s: 1, peak_commit_mb: 1, written_mb: 0, exit: 0, warmup });
  const rows = medians([run('b', 999, true), run('b', 3), run('a', 9), run('b', 1), run('b', 2), run('a', 7)]);
  assert.deepEqual(rows.map((r) => [r.label, r.runs, r.wall_s]), [['b', 3, 2], ['a', 2, 8]]);
  assert.equal(median([5, 1, 3]), 3);
  assert.match(table(rows), /^\| Run \|.*\n\|---/);
});

test('both sides run every matched scenario, with no shell and Faber one test file at a time', () => {
  const steps = plan('matched', 'T');
  for (const m of MATCHED) {
    assert.ok(steps.some((s) => s.dir === 'T' && s.scenario === m.id && s.program === 'node'), `TL-ADE ${m.id}`);
    assert.equal(steps.filter((s) => s.program === 'cargo' && s.scenario === m.id).length, m.faber.length, `Faber ${m.id}`);
  }
  assert.ok(steps.every((s) => ['node', 'cargo'].includes(s.program)));
  assert.deepEqual(plan('suites', 'T').map((s) => s.label), ['tlade suite', 'faber suite']);
});
