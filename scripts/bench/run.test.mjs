import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MATCHED, REAL_WORK, median, medians, notes, plan, table } from './run.mjs';

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

test('each real-work task is one screen run of its own, and its notes are read back as numbers', () => {
  const steps = plan('real-work', 'T');
  const faber = steps.filter((s) => s.program === 'cargo');
  assert.deepEqual(faber.map((s) => s.args.at(-1)), REAL_WORK.map((t) => `real work ${t.id}:`));
  const other = steps.filter((s) => s.program === 'node');
  assert.deepEqual(other.map((s) => s.task), [1, 2, 3, 4, 6], 'the other side runs the tasks it can do');
  assert.ok(other.every((s) => s.args.includes('--standin') && s.notes));
  const log = [
    'pass  screen  real work 1: sitting idle (61.2 s)  [open 1.20 s, app 180 MB]',
    'FAIL  screen  real work 2: one chat turn (3.0 s)',
  ].join('\n');
  assert.deepEqual(notes(log, 1), { open_s: 1.2, app_mb: 180 });
  assert.equal(notes(log, 2), null, 'a failed run has no notes');
  assert.equal(notes('pass  screen  real work 10: x  [a 1 s]', 1), null, 'task 1 is not task 10');
});

test('the other side is fed what Faber replays: the same answers, the same 50 turns and a receipt it trusts', async () => {
  const { TASKS, answerOf, receipt, thread } = await import('./tlade.mjs');
  assert.match(answerOf('tests/fixtures/claude/chat.jsonl'), /Rayleigh/);
  assert.equal(answerOf('tests/fixtures/claude/resume.jsonl'), 'pong');
  const turns = thread(50).turns;
  assert.equal(turns.length, 100);
  assert.deepEqual([turns[0].role, turns[1].role], ['user', 'assistant']);
  assert.equal(turns[0].text, 'Question 0: why is the sky blue?');
  assert.equal(new Set(turns.map((t) => t.id)).size, 100);
  const now = Date.parse('2026-10-22T12:00:00Z');
  const r = receipt('claude', 47, now);
  assert.equal(r.source, 'official');
  assert.ok(Date.parse(r.weekly_reset_at) > now && Date.parse(r.observed_at) === now);
  assert.ok(!TASKS.includes(5) && !TASKS.includes(7), 'tasks it cannot do are listed, not measured');
});
