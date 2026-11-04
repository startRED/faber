import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { EventEmitter } from 'node:events';
import { PassThrough, Writable } from 'node:stream';
import { readFileSync } from 'node:fs';

process.env.FABER_SIDECAR = 'fake-sidecar';
const budget = Number(readFileSync(new URL('./main.ts', import.meta.url), 'utf8').match(/const HOST_STOP_MS = ([\d_]+)/)[1].replaceAll('_', ''));

const children = [];
globalThis.sidecarTest = {
  app: { isPackaged: false, getVersion: () => 'test' },
  spawn() {
    const child = new EventEmitter();
    child.requests = [];
    child.killed = false;
    child.stdout = new PassThrough();
    child.stdin = new Writable({ write(line, _encoding, done) {
      child.requests.push(JSON.parse(line)); done();
    } });
    child.kill = () => { child.killed = true; queueMicrotask(() => child.emit('exit', 0)); };
    children.push(child);
    return child;
  },
};
registerHooks({ resolve(specifier, context, next) {
  if (specifier === 'electron' || specifier === 'node:child_process' || specifier === './orders.ts') return { url: `mock:${specifier}`, shortCircuit: true };
  return next(specifier, context);
}, load(url, context, next) {
  const sources = {
    'mock:electron': 'export const { app } = globalThis.sidecarTest;',
    'mock:node:child_process': 'export const { spawn } = globalThis.sidecarTest;',
    'mock:./orders.ts': 'export const LOGIN_ITEM = "Faber";',
  };
  return sources[url] ? { format: 'module', source: sources[url], shortCircuit: true } : next(url, context);
} });
const { Sidecar } = await import('./sidecar.ts');
const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
const listeners = () => ({ event() {}, order() {} });

test('quit waits beyond five seconds for the engine to finish saving', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const host = Sidecar.start(listeners());
  const child = children.at(-1);
  let ended = false;
  const stopping = host.stop(budget).then(() => { ended = true; });
  t.mock.timers.tick(5001); await flush();
  assert.equal(child.killed, false, 'slow saves must not be killed at five seconds');
  assert.equal(ended, false, 'Quit must wait for the engine exit');
  child.emit('exit', 0);
  await stopping;
});

test('quit bounds a hung host and waits for its exit after termination', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const host = Sidecar.start(listeners());
  const child = children.at(-1);
  child.kill = () => { child.killed = true; };
  let ended = false;
  const stopping = host.stop(budget).then(() => { ended = true; });
  t.mock.timers.tick(60_000); await flush();
  assert.equal(child.killed, true);
  assert.equal(ended, false, 'termination must finish before the shell quits');
  child.emit('exit', 0);
  await stopping;
});

test('repeated crashes back off and Quit cancels a pending restart', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const host = Sidecar.start(listeners());
  let child = children.at(-1);
  child.emit('exit', 1);
  t.mock.timers.tick(999); await flush();
  assert.equal(children.at(-1), child);
  t.mock.timers.tick(1); await flush();
  const replacement = children.at(-1);
  assert.notEqual(replacement, child);
  child = replacement;
  child.emit('exit', 1);
  t.mock.timers.tick(1999); await flush();
  assert.equal(children.at(-1), child, 'unhealthy launches must not restart in a tight loop');
  await host.stop(budget);
  t.mock.timers.tick(60_000); await flush();
  assert.equal(children.at(-1), child);
});

test('a crashed engine restarts, refreshes the screen and never replays requests', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const events = [];
  const host = Sidecar.start({ ...listeners(), event: (event) => events.push(event) });
  const crashed = children.at(-1);
  const pending = assert.rejects(host.call('send', { text: 'only once' }), /error.host.gone/);
  crashed.emit('exit', 1);
  await pending;
  t.mock.timers.tick(1000); await flush();
  const replacement = children.at(-1);
  assert.notEqual(replacement, crashed, 'the dead host must be replaced');
  assert.deepEqual(replacement.requests.map((request) => request.cmd), ['state']);
  replacement.stdout.write(JSON.stringify({ id: replacement.requests[0].id, ok: {} }) + '\n');
  await flush();
  assert.deepEqual(events, [{ type: 'lagged' }], 'reload only after the replacement answers');
  const stopping = host.stop(budget); replacement.emit('exit', 0); await stopping;
  t.mock.timers.tick(60_000); await flush();
  assert.equal(children.at(-1), replacement, 'Quit never starts another host');
});
