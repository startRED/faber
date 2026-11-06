import assert from 'node:assert/strict';
import test from 'node:test';
import { CAP_USD, CODEX_EVIDENCE, LIMIT_MS, POLL_MS, QUIET_MS, codexWeekly, cpuPercent, cpuSnapshot, killArgs, quietSince, subscriptionEnv, verifiedQuota, watch } from './bench-when-quiet.mjs';

test('Codex workspaces stay outside credential-folder ancestors even when the checkout is inside .claude', () => {
  assert.equal(CODEX_EVIDENCE.split(/[\\/]/).some((part) => ['.claude', '.codex', '.gemini', '.ssh', '.aws'].includes(part.toLowerCase())), false);
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
