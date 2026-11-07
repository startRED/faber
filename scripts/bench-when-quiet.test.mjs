import assert from 'node:assert/strict';
import test from 'node:test';
import { CAP_USD, CLAUDE_DEADLINE, CLAUDE_EVIDENCE, CLAUDE_RESET, CODEX_EVIDENCE, LIMIT_MS, POLL_MS, QUIET_MS, claudeReport, claudeWeekly, codexWeekly, cpuPercent, cpuSnapshot, killArgs, quietSince, subscriptionEnv, subscriptionRequests, verifiedQuota, watch, watcherOptions } from './bench-when-quiet.mjs';

test('Codex workspaces stay outside credential-folder ancestors even when the checkout is inside .claude', () => {
  for (const folder of [CODEX_EVIDENCE, CLAUDE_EVIDENCE])
    assert.equal(folder.split(/[\\/]/).some((part) => ['.claude', '.codex', '.gemini', '.ssh', '.aws'].includes(part.toLowerCase())), false);
});

test('Claude scheduling cannot precede reset, exceed deadline or mix vendor modes; invalid times fail closed', () => {
  assert.deepEqual(watcherOptions([]), { codexOnly: false, claudeOnly: false, notBefore: 0 });
  assert.deepEqual(watcherOptions(['--codex-only']), { codexOnly: true, claudeOnly: false, notBefore: 0 });
  for (const args of [[], ['--not-before', '2026-11-07T08:00:00Z']])
    assert.equal(watcherOptions(['--claude-only', ...args]).notBefore, CLAUDE_RESET);
  assert.equal(watcherOptions(['--claude-only', '--not-before', '2026-11-11T08:00:00Z']).notBefore, CLAUDE_RESET + 86400_000);
  for (const args of [
    ['--claude-only', '--codex-only'], ['--claude-only', '--not-before', '2026-11-12T00:00:00Z'],
    ['--not-before'], ['--not-before', 'bad'], ['--not-before', '2026-11-31T08:00:00Z'],
    ['--not-before', '2026-11-10'], ['--not-before', '2026-11-10T08:00:00Z', '--not-before', '2026-11-11T08:00:00Z'],
    ['--not-before', '1970-01-01T00:00:00Z', '--not-before', '2026-11-10T08:00:00Z'], ['--typo'],
  ]) assert.throws(() => watcherOptions(args));
});

test('scheduled polling does no sampling or work before reset and requires ten fresh quiet minutes afterward', async () => {
  let at = CLAUDE_RESET - 3 * POLL_MS - 123;
  let samples = 0;
  const pauses = [];
  assert.equal(await watch({
    now: () => at, notBefore: CLAUDE_RESET, deadline: CLAUDE_DEADLINE,
    pause: async (ms) => { assert.ok(ms > 0 && ms <= POLL_MS); pauses.push(ms); at += ms; },
    sample: async () => { assert.ok(at >= CLAUDE_RESET); samples++; return { at, cpu: 2, game: false, gameAt: 0 }; },
    record() {}, run: async (deadline) => { assert.equal(at, CLAUDE_RESET + QUIET_MS); assert.equal(deadline, CLAUDE_DEADLINE); return 'finished'; },
  }), 'finished');
  assert.equal(samples, 11);
  assert.deepEqual(pauses.slice(0, 4), [POLL_MS, POLL_MS, POLL_MS, 123]);
});

test('absolute deadline applies during scheduled waiting and during quiet admission, without a 14-hour cutoff', async () => {
  for (const notBefore of [CLAUDE_DEADLINE + 1, CLAUDE_RESET]) {
    let at = CLAUDE_RESET - 48 * 3600_000;
    assert.equal(await watch({
      now: () => at, notBefore, deadline: CLAUDE_DEADLINE,
      pause: async (ms) => { assert.ok(ms <= POLL_MS); at += ms; },
      sample: async () => { assert.ok(at >= notBefore); return { at, cpu: 90, game: false }; },
      record() {}, run: async () => assert.fail('no work admitted'),
    }), 'deadline');
    assert.equal(at, CLAUDE_DEADLINE);
  }
});

function claudeReceipt(used = 0.2) {
  return { readings: [{ vendor: 'claude', source: 'app claude_limits (control-only get_usage, zero model cost)',
    paidFallback: false, completedAt: CLAUDE_RESET / 1000 + 60, windows: [{ window: 'seven_day', used, resetsAt: (CLAUDE_RESET + 7 * 86400_000) / 1000 }] }] };
}

test('Claude admission requires the fixed reader and exactly one current new-week weekly window', () => {
  const now = CLAUDE_RESET + 120_000;
  assert.equal(claudeWeekly(claudeReceipt(0.25), now), 0.25);
  for (const change of [
    (r) => { r.readings = []; }, (r) => { r.readings[0].windows = []; },
    (r) => { r.readings[0].windows.push(r.readings[0].windows[0]); },
    (r) => { r.readings[0].source = 'claude_usage'; }, (r) => { r.readings[0].vendor = 'codex'; },
    (r) => { r.readings[0].completedAt = CLAUDE_RESET / 1000 - 1; },
    (r) => { r.readings[0].completedAt = now / 1000 + 1; }, (r) => { delete r.readings[0].completedAt; },
    (r) => { r.readings[0].windows[0].resetsAt = CLAUDE_RESET / 1000; },
    (r) => { r.readings[0].windows[0].resetsAt += 86400; },
    (r) => { r.readings[0].unknownCost = true; }, (r) => { r.readings[0].windows[0].used = -1; },
    (r) => { r.readings[0].paidFallback = true; }, (r) => { delete r.readings[0].paidFallback; },
  ]) { const invalid = claudeReceipt(); change(invalid); assert.throws(() => claudeWeekly(invalid, now)); }
});

test('subscription requests preserve Codex sequencing and enforce Claude 25% before/after every request', async () => {
  for (const claudeOnly of [false, true]) {
    for (const stopAt of [0, 1, 16]) {
      const calls = [], receipts = [];
      let readings = 0;
      const vendor = claudeOnly ? 'claude' : 'codex';
      const result = await subscriptionRequests({
        claudeOnly, attempt: 1, evidence: 'fake', receipts, record() {}, now: () => CLAUDE_RESET + 120_000,
        invoke: async (args, name, deadline) => {
          assert.equal(deadline, CLAUDE_DEADLINE); assert.equal(args.includes(`--${vendor}-only`), true);
          assert.equal(args.includes(`--${claudeOnly ? 'codex' : 'claude'}-only`), false);
          assert.equal(args.includes('--require-hard-cap'), false); calls.push(name); return { code: 0 };
        },
        read: async (file) => {
          if (file.endsWith('results.json')) return JSON.stringify({ rows: [{ plain: {}, faber: {} }] });
          const receipt = claudeReceipt(readings++ === stopAt ? (claudeOnly ? 0.251 : 0.701) : (claudeOnly ? 0.25 : 0.70));
          if (!claudeOnly) { receipt.readings[0].vendor = 'codex'; receipt.readings[0].source = 'app account/rateLimits/read'; }
          return JSON.stringify(receipt);
        },
      }, CLAUDE_DEADLINE);
      assert.equal(result, stopAt === 16 ? 'finished' : `stopped: ${vendor} weekly ${claudeOnly ? '25.1% used exceeds 25%' : '70.1% used exceeds 70%'}`);
      assert.equal(calls.length, stopAt === 0 ? 1 : stopAt === 1 ? 3 : 24);
      assert.equal(receipts.length, stopAt === 16 ? 16 : stopAt + 1);
      assert.equal(calls[0], 'attempt-1-organize-folder-before');
      if (stopAt !== 0) assert.deepEqual(calls.slice(0, 3), ['before', 'bench', 'after'].map((phase) => `attempt-1-organize-folder-${phase}`));
    }
  }
});

test('missing Claude quota stops before model work; game/deadline stops never start another phase', async () => {
  for (const reason of [null, 'game', 'deadline', 'guard-failed']) {
    let calls = 0;
    const run = () => subscriptionRequests({
      claudeOnly: true, attempt: 1, evidence: 'fake', receipts: [], record() {}, now: () => CLAUDE_RESET + 120_000,
      invoke: async () => { calls++; return { code: 0, reason }; },
      read: async () => JSON.stringify({ readings: [] }),
    }, CLAUDE_DEADLINE);
    if (!reason) await assert.rejects(run, /quota receipt missing/);
    else assert.equal(await run(), reason === 'game' ? 'stopped: cs2 started' : reason);
    assert.equal(calls, 1);
  }
});

test('the eight-row report distinguishes missing readings, interrupted tokens, partial results and real verdicts', () => {
  const summary = { result: 'stopped: claude weekly 25.1% used exceeds 25%', started: 'start', ended: 'end',
    notBefore: 'reset', deadline: 'deadline', receipts: [{ request: 'organize-folder', phase: 'before', ...claudeReceipt(0.251) },
      { request: 'plan-an-app', phase: 'before', readings: [{ error: 'no window', windows: [] }] }],
    comparisons: [{ request: 'organize-folder', secs: 120 }, { request: 'plan-an-app' }] };
  const results = [{ rows: [{ request: 'organize-folder', winner: 'faber', verdict: { reason: 'better | proof' },
    plain: { usage: { input: 1, output: 2, cache_read: 3, cache_write: 4 }, wall_secs: 50, proof: { pass: true } },
    faber: { usage: { input: 5 }, wall_secs: 60, proof: { pass: false } }, judge_attempted: true, judge_usage: { output: 6 } }] }];
  const report = claudeReport(summary, results, 'fake-evidence');
  assert.equal(report.split('\n').filter((l) => /^\| (organize|plan|todo|research|everyday|fix|migrate|summarize)/.test(l)).length, 8);
  assert.match(report, /faber: better \\\| proof/);
  assert.match(report, /pass \/ fail \| 10 \/ 5 \/ 6 \| 50 \/ 60 \| 120/);
  assert.match(report, /25.1%; read 2026-11-10T08:01:00.000Z; resets 2026-11-17T08:00:00.000Z/);
  assert.match(report, /plan-an-app \| no verdict; interrupted or incomplete \| unavailable \/ unavailable \| unknown/);
  assert.match(report, /unavailable: no window/);
  assert.match(report, /todo-app \| not started/);
  assert.match(report, /partial/);
  assert.match(report, /Stop reason: stopped: claude weekly 25.1%/);
});

test('ten consecutive idle minutes are required; game, busy CPU, bad samples and sleep reset admission', () => {
  const state = { at: POLL_MS, cpu: 14.9, game: false, gameAt: 0 };
  assert.equal(quietSince(null, state, 0), 0);
  for (const patch of [{ cpu: 15 }, { cpu: NaN }, { cpu: -Infinity }, { game: true }, { gameAt: 1 }, { at: 2 * POLL_MS }, { at: 0 }]) {
    assert.equal(quietSince(0, { ...state, ...patch }, 0), null);
  }
  assert.equal(cpuPercent({ idle: 100, total: 100 }, { idle: 185, total: 200 }), 15.000000000000002);
  assert.ok(Number.isNaN(cpuPercent({ idle: 1, total: 1 }, { idle: 1, total: 1 })));
  assert.deepEqual(cpuSnapshot([{ times: { idle: 10, user: 2, sys: 3 } }]), { idle: 10, total: 15 });
});

test('a game interruption resumes waiting, then needs a new full ten-minute quiet interval', async () => {
  let at = 0;
  let calls = 0;
  const starts = [];
  const result = await watch({
    now: () => at,
    pause: async (ms) => { at += ms; },
    sample: async () => ({ at, cpu: 2, game: false, gameAt: 0 }),
    record() {},
    run: async (deadline) => { assert.equal(deadline, LIMIT_MS); starts.push(at); return ++calls === 1 ? 'game' : 'finished'; },
  });
  assert.deepEqual(starts, [QUIET_MS, 2 * QUIET_MS]);
  assert.equal(result, 'finished');
});

test('a game already open at startup cannot count as the first idle minute', async () => {
  let at = 0;
  const starts = [];
  await watch({
    now: () => at, pause: async (ms) => { at += ms; },
    sample: async () => ({ at, cpu: 2, game: at === 0, gameAt: 0 }), record() {},
    run: async () => { starts.push(at); return 'finished'; },
  });
  assert.deepEqual(starts, [QUIET_MS + POLL_MS]);
});

test('the 14-hour limit starts no bench when the PC never becomes quiet', async () => {
  let at = 0;
  assert.equal(await watch({
    now: () => at, pause: async (ms) => { at += ms; },
    sample: async () => ({ at, cpu: 90, game: true }), record() {},
    run: async () => assert.fail('busy PC must not start work'),
  }), '14-hour limit');
  assert.equal(at, LIMIT_MS);
});

test('billing variables are removed case-insensitively without changing the parent environment', () => {
  const parent = { Path: 'tools', ANTHROPIC_API_KEY: 'fake', openai_api_key: 'fake', CLAUDE_CODE_USE_VERTEX: '1', CODEX_API_KEY: 'fake',
    AZURE_OPENAI_API_KEY: 'fake', OPENAI_BILLING_PROJECT: 'fake', OPENAI_PROJECT_ID: 'fake', OPENAI_ORG_ID: 'fake',
    OPENAI_ORGANIZATION: 'fake', OPENAI_PROJECT: 'fake', AWS_SECRET_ACCESS_KEY: 'fake', CODEX_BEARER_TOKEN: 'fake', TERM: 'x' };
  assert.deepEqual(subscriptionEnv(parent), { Path: 'tools', TERM: 'x' });
  assert.equal(parent.ANTHROPIC_API_KEY, 'fake');
  assert.equal(CAP_USD, 25);
});

test('Codex-only weekly admission accepts an unpaired app reading and rejects missing or invalid windows', () => {
  const receipt = { readings: [{ vendor: 'codex', source: 'app account/rateLimits/read', completedAt: 100,
    windows: [{ window: 'seven_day', used: 0.43, resetsAt: 1000 }] }] };
  assert.equal(codexWeekly(receipt), 0.43);
  receipt.readings[0].windows[0].used = 0.71;
  assert.ok(codexWeekly(receipt) > 0.70);
  receipt.readings[0].windows[0].window = 'session';
  assert.throws(() => codexWeekly(receipt), /weekly quota/);
  receipt.readings[0].windows[0].window = 'seven_day';
  receipt.readings[0].windows[0].used = NaN;
  assert.throws(() => codexWeekly(receipt), /invalid/);
  receipt.readings.push({ vendor: 'claude' });
  assert.throws(() => codexWeekly(receipt), /invalid/);
});

test('tree termination accepts only an owned positive PID, never an image name', () => {
  assert.deepEqual(killArgs(1234), ['/PID', '1234', '/T', '/F']);
  for (const pid of [0, -1, 1.5, 'cs2.exe', '1234', NaN, Infinity]) assert.throws(() => killArgs(pid));
});

test('unverified, duplicate, missing or spent quota receipts cannot authorize another reading', () => {
  const receipt = { readings: ['claude', 'codex'].map((vendor) => ({ vendor, source: 'local', completedAt: 100,
    windows: [{ window: 'session', used: 0.2, resetsAt: 1000 }] })) };
  assert.equal(verifiedQuota(receipt), true);
  for (const change of [
    (r) => { r.readings[0].unknownCost = true; },
    (r) => { r.readings[0].error = 'unknown'; },
    (r) => { r.readings[0].windows = []; },
    (r) => { r.readings[0].windows[0].resetsAt = 100; },
    (r) => { r.readings[0].windows[0].used = -1; },
    (r) => { r.readings[0].vendor = 'codex'; },
    (r) => { r.readings.pop(); },
  ]) {
    const invalid = structuredClone(receipt); change(invalid);
    assert.equal(verifiedQuota(invalid), false);
  }
});
