import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { EventEmitter, once } from 'node:events';
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
const listeners = () => ({ event() {}, order() {}, ended() {} });

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

test('an exited host with a pipe still open rejects unanswered requests within one second', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const ended = [];
  const events = [];
  const host = Sidecar.start({ ...listeners(), ended: (code) => ended.push(code), event: (event) => events.push(event) });
  const child = children.at(-1);
  const pending = assert.rejects(host.call('send', { text: 'only once' }), /error.host.gone/);
  child.emit('exit', 1);
  await assert.rejects(host.call('state', {}), /error.host.gone/);
  t.mock.timers.tick(999);
  assert.deepEqual(ended, []);
  t.mock.timers.tick(1);
  await pending;
  child.stdout.write(JSON.stringify({ event: { type: 'lagged' } }) + '\n');
  child.emit('error', new Error('late error after exit'));
  child.emit('exit', 1);
  child.emit('close', 1);
  t.mock.timers.tick(60_000); await flush();
  assert.deepEqual(ended, [1]);
  assert.deepEqual(events, [], 'the exited host cannot publish stale events');
  assert.equal(children.at(-1), child, 'only main.ts may start a replacement');
  assert.deepEqual(child.requests.map((request) => request.cmd), ['send'], 'requests are never replayed');
  await host.stop(budget);
});

test('a failed host can deliver a buffered response after its exit notification', async () => {
  const ended = [];
  const host = Sidecar.start({ event: assert.fail, order: assert.fail, ended: (code) => ended.push(code) });
  const child = children.at(-1);
  const pending = host.call('state', {}).catch((error) => error.message);
  child.emit('exit', 1);
  assert.deepEqual(ended, []);
  child.stdout.write('{"event":{"type":"lagged"}}\n{"order":{"kind":"show"}}\n');
  child.stdout.write(JSON.stringify({ id: child.requests[0].id, ok: { saved: true } }) + '\n');
  assert.deepEqual(ended, [1], 'the last buffered reply permits immediate recovery');
  child.emit('close', 1);
  assert.deepEqual(await pending, { saved: true });
  assert.deepEqual(ended, [1]);
  assert.deepEqual(child.requests.map((request) => request.cmd), ['state']);
});

test('Quit during a reply drain never starts a replacement', async () => {
  const ended = [];
  const host = Sidecar.start({ ...listeners(), ended: (code) => ended.push(code) });
  const child = children.at(-1);
  const pending = host.call('state', {});
  child.emit('exit', 1);
  const stopping = host.stop(budget);
  child.stdout.write(JSON.stringify({ id: child.requests[0].id, ok: true }) + '\n');
  assert.equal(await pending, true);
  await stopping;
  assert.deepEqual(ended, []);
  assert.equal(child.killed, false);
});

test('Quit begun before exit drains the final reply without trying to kill the exited host', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const ended = [];
  const host = Sidecar.start({ ...listeners(), ended: (code) => ended.push(code) });
  const child = children.at(-1);
  const pending = host.call('state', {}).catch((error) => error.message);
  const stopping = host.stop(50);
  t.mock.timers.tick(49);
  child.emit('exit', 0);
  t.mock.timers.tick(1);
  await flush();
  assert.equal(child.killed, false);
  child.stdout.write(JSON.stringify({ id: child.requests[0].id, ok: true }) + '\n');
  assert.equal(await pending, true);
  await stopping;
  assert.deepEqual(ended, []);
});

test('exit without pending calls can restart immediately even with a pipe open', () => {
  const ended = [];
  Sidecar.start({ ...listeners(), ended: (code) => ended.push(code) });
  const child = children.at(-1);
  child.emit('exit', 1);
  assert.deepEqual(ended, [1]);
});

test('an already drained pipe cannot keep an unanswered call pending after exit', async () => {
  const ended = [];
  const host = Sidecar.start({ ...listeners(), ended: (code) => ended.push(code) });
  const child = children.at(-1);
  const pending = assert.rejects(host.call('state', {}), /error.host.gone/);
  const drained = once(child.stdout, 'end');
  child.stdout.end();
  await drained;
  child.emit('exit', 1);
  assert.deepEqual(ended, [1]);
  await pending;
});

test('a non-object line does not break the next host reply', async () => {
  const host = Sidecar.start(listeners());
  const child = children.at(-1);
  const pending = host.call('state', {});
  child.stdout.write('null\n42\n');
  child.stdout.write(JSON.stringify({ id: child.requests[0].id, ok: { connected: true } }) + '\n');
  assert.deepEqual(await pending, { connected: true });
  child.emit('exit', 0);
  child.emit('close', 0);
});

test('an unrequested clean exit is reported to the restart owner', () => {
  const ended = [];
  Sidecar.start({ ...listeners(), ended: (code) => ended.push(code) });
  const child = children.at(-1);
  child.emit('exit', 0);
  child.emit('close', 0);
  assert.deepEqual(ended, [0]);
});

test('a launch error is reported once even if an exit follows', () => {
  const ended = [];
  Sidecar.start({ ...listeners(), ended: (code) => ended.push(code) });
  const child = children.at(-1);
  child.emit('error', new Error('missing host'));
  child.emit('exit', 1);
  assert.deepEqual(ended, [null]);
});

test('Quit refuses new calls and never notifies the restart owner', async () => {
  const ended = [];
  const host = Sidecar.start({ ...listeners(), ended: (code) => ended.push(code) });
  const child = children.at(-1);
  const stopping = host.stop(budget);
  await assert.rejects(host.call('send', { text: 'too late' }), /error.host.gone/);
  child.emit('exit', 0);
  await stopping;
  assert.deepEqual(ended, []);
});
