import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';

const source = stripTypeScriptTypes(readFileSync(new URL('./main.ts', import.meta.url), 'utf8'));
const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };

function shell() {
  const hosts = [];
  const sent = [];
  let now = 0;
  let quits = 0;
  let reloads = 0;
  const context = {
    Date: { now: () => now },
    console: { error() {} },
    quitNow: () => quits++,
    win: {
      isVisible: () => true, isMinimized: () => false, isFocused: () => true,
      webContents: { reload: () => reloads++, send: (...args) => sent.push(args) },
    },
    Sidecar: { start(listen) {
      const ready = Promise.withResolvers();
      const host = { listen, ready, calls: [], notes: [],
        call(cmd, args) { host.calls.push({ cmd, args }); return ready.promise; },
        note(note) { host.notes.push(note); },
      };
      hosts.push(host);
      return host;
    } },
  };
  const control = runInNewContext(`let host = null; let stopping = false;
    ${source.slice(source.indexOf('const restarts'))}
    host = startHost({});
    ({ stop: () => { stopping = true; } });`, context);
  return { hosts, sent, control, advance: (ms) => { now += ms; }, quits: () => quits, reloads: () => reloads };
}

test('main restarts a failed host and recovers the live renderer only after a state reply', async () => {
  const s = shell();
  s.hosts[0].listen.ended(1);
  assert.equal(s.hosts.length, 2);
  const replacement = s.hosts[1];
  assert.deepEqual(replacement.calls.map((call) => call.cmd), ['state']);
  assert.equal(replacement.notes[0].visible, true);
  assert.equal(replacement.notes[1].focused, true);
  assert.equal(s.sent.length, 0, 'the replacement must answer before recovery');
  replacement.ready.resolve({});
  await flush();
  assert.equal(s.sent.length, 1);
  assert.equal(s.sent[0][0], 'faber:event');
  assert.equal(s.sent[0][1].type, 'lagged');
  assert.equal(s.reloads(), 0, 'recovery must retain drafts whose storage writes failed');
  assert.equal(s.quits(), 0);
});

test('lost host events use the same live-renderer recovery path', () => {
  const s = shell();
  s.hosts[0].listen.event({ type: 'lagged' });
  assert.equal(s.sent[0][0], 'faber:event');
  assert.equal(s.sent[0][1].type, 'lagged');
  assert.equal(s.reloads(), 0);
});

test('main closes on an unrequested clean exit or after three rapid restarts', async () => {
  const clean = shell();
  clean.hosts[0].listen.ended(0);
  assert.equal(clean.quits(), 1);
  assert.equal(clean.hosts.length, 1);
  const s = shell();
  for (let i = 0; i < 3; i++) {
    s.hosts.at(-1).listen.ended(i === 0 ? null : 1);
    s.hosts.at(-1).ready.resolve({});
    await flush();
    s.advance(1_000);
  }
  s.hosts.at(-1).listen.ended(1);
  assert.equal(s.hosts.length, 4, 'short-lived state replies never reset the restart limit');
  assert.equal(s.quits(), 1);
});

test('thirty seconds of host uptime resets the failure window, shorter uptime does not', () => {
  for (const uptime of [29_999, 30_000]) {
    const s = shell();
    for (let i = 0; i < 3; i++) s.hosts.at(-1).listen.ended(1);
    s.advance(uptime);
    s.hosts.at(-1).listen.ended(1);
    assert.equal(s.hosts.length, uptime < 30_000 ? 4 : 5);
    assert.equal(s.quits(), uptime < 30_000 ? 1 : 0);
  }
});

test('restart counts expire after a minute even without thirty seconds of uptime', () => {
  const s = shell();
  for (let i = 0; i < 4; i++) {
    s.advance(20_001);
    s.hosts.at(-1).listen.ended(1);
  }
  assert.equal(s.hosts.length, 5);
  assert.equal(s.quits(), 0);
});

test('failed or superseded readiness and Quit never publish recovery', async () => {
  const s = shell();
  s.hosts[0].listen.ended(1);
  const stale = s.hosts.at(-1);
  stale.listen.ended(1);
  stale.ready.resolve({});
  s.hosts.at(-1).ready.reject(new Error('error.host.gone'));
  await flush();
  assert.equal(s.sent.length, 0);
  s.hosts.at(-1).listen.ended(1);
  s.control.stop();
  s.hosts.at(-1).ready.resolve({});
  s.hosts.at(-1).listen.ended(0);
  await flush();
  assert.equal(s.sent.length, 0);
  assert.equal(s.hosts.length, 4);
  assert.equal(s.quits(), 0);
});
