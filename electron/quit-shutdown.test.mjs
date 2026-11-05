import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { registerHooks, stripTypeScriptTypes } from 'node:module';
import { PassThrough } from 'node:stream';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

let child;
process.env.FABER_SIDECAR = 'fake-sidecar';
globalThis.sidecarTestSpawn = () => child;
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'electron' || specifier === 'node:child_process' || specifier === './orders.ts') {
      return { url: `sidecar-test:${specifier}`, shortCircuit: true };
    }
    return next(specifier, context);
  },
  load(url, context, next) {
    const sources = {
      'sidecar-test:electron': "export const app = { isPackaged: false, getVersion: () => 'test' };",
      'sidecar-test:node:child_process': 'export const spawn = (...args) => globalThis.sidecarTestSpawn(...args);',
      'sidecar-test:./orders.ts': "export const LOGIN_ITEM = 'Faber';",
    };
    if (url in sources) return { format: 'module', source: sources[url], shortCircuit: true };
    return next(url, context);
  },
});
const { Sidecar } = await import('./sidecar.ts');
const main = readFileSync(new URL('./main.ts', import.meta.url), 'utf8');
const patience = Number(main.match(/const HOST_STOP_MS = ([\d_]+);/)[1].replaceAll('_', ''));

function start(t) {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  child = Object.assign(new EventEmitter(), { stdin: new PassThrough(), stdout: new PassThrough(), killed: false, pid: 1234 });
  const process = child;
  process.kill = () => { process.killed = true; return true; };
  t.after(() => { process.emit('exit', 0); process.stdout.end(); });
  const ended = [];
  const host = Sidecar.start({ event() {}, order() {}, ended: (code) => ended.push(code) });
  return { host, process, ended };
}
const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };

test('Quit preserves a save that finishes after the former five-second crash point', async (t) => {
  const { host, process } = start(t);
  let saved = false;
  let stopped = false;
  const stopping = host.stop(patience).then(() => { stopped = true; });
  assert.equal(process.stdin.writableEnded, true, 'EOF asks the engine to flush and shut down');
  setTimeout(() => { saved = true; process.emit('exit', 0); }, 6000);
  t.mock.timers.tick(5001);
  await settle();
  assert.equal(process.killed, false, 'a slow final save must survive the old kill deadline');
  assert.equal(stopped, false);
  t.mock.timers.tick(999);
  await stopping;
  assert.equal(saved, true);
  assert.equal(process.killed, false);
});

test('Quit waits for actual engine exit after the longer hang deadline', async (t) => {
  const { host, process } = start(t);
  assert.ok(patience >= 60_000, 'allow slow storage a full minute before force termination');
  let stopped = false;
  const stopping = host.stop(patience).then(() => { stopped = true; });
  t.mock.timers.tick(patience - 1);
  await settle();
  assert.equal(process.killed, false);
  t.mock.timers.tick(1);
  await settle();
  assert.equal(process.killed, true, 'a hung engine is terminated only at the bound');
  assert.equal(stopped, false, 'requesting termination is not confirmation of exit');
  process.emit('exit', 1);
  await stopping;
});

test('a repeated Quit cannot bypass a pending engine save', async () => {
  const app = new EventEmitter();
  let finish;
  let stops = 0;
  let quits = 0;
  app.quit = () => { quits++; app.emit('before-quit', { preventDefault() { assert.fail('confirmed exit should permit Quit'); } }); };
  const host = { stop: () => { stops++; return new Promise((resolve) => { finish = resolve; }); } };
  const source = main.slice(main.indexOf('let stopping = false;'), main.indexOf('void app.whenReady()'));
  runInNewContext(stripTypeScriptTypes(source), { app, host, HOST_STOP_MS: patience, release() {}, pane: { closeAll() {} } });
  let prevented = 0;
  const event = { preventDefault() { prevented++; } };
  app.emit('before-quit', event);
  app.emit('before-quit', event);
  assert.equal(prevented, 2, 'every Quit is held until the engine exits');
  assert.equal(stops, 1);
  assert.equal(quits, 0);
  finish();
  await settle();
  assert.equal(quits, 1);
});

test('requesting force termination never acknowledges engine exit', async (t) => {
  const { host, process } = start(t);
  let stopped = false;
  const stopping = host.stop(60_000).then(() => { stopped = true; });
  t.mock.timers.tick(60_000);
  await settle();
  assert.equal(process.killed, true);
  assert.equal(stopped, false, 'the shell must still wait for the exit event');
  process.emit('exit', 1);
  await stopping;
});

test('failed termination keeps the tray and allows another Quit to retry', async () => {
  const app = new EventEmitter();
  let fail;
  let finish;
  let stops = 0;
  let quits = 0;
  let released = 0;
  let closed = 0;
  const failures = [];
  app.quit = () => { quits++; };
  const host = { stop: () => {
    stops++;
    return new Promise((resolve, reject) => { fail = reject; finish = resolve; });
  } };
  const source = main.slice(main.indexOf('let stopping = false;'), main.indexOf('void app.whenReady()'));
  runInNewContext(stripTypeScriptTypes(source), {
    app, host, HOST_STOP_MS: patience, release: () => { released++; },
    pane: { closeAll() { closed++; } }, console: { error: (...args) => failures.push(args) },
  });
  const event = { preventDefault() {} };
  app.emit('before-quit', event);
  assert.equal(released, 0, 'the tray stays usable until exit is confirmed');
  assert.equal(closed, 0, 'failed Quit must preserve open panes');
  const failure = new Error('kill denied');
  fail(failure);
  await settle();
  assert.equal(quits, 0);
  assert.equal(released, 0);
  assert.equal(closed, 0);
  assert.equal(failures.length, 1);
  assert.equal(failures[0][1], failure, 'report the failed shutdown without quitting');
  app.emit('before-quit', event);
  assert.equal(stops, 2, 'Quit can retry a failed termination');
  finish();
  await settle();
  assert.equal(quits, 1);
  assert.equal(released, 1);
  assert.equal(closed, 1);
});

test('a failed kill is reported instead of acknowledging an engine still saving', async (t) => {
  const { host, process } = start(t);
  process.kill = () => false;
  const stopping = host.stop(60_000);
  void stopping.catch(() => {});
  t.mock.timers.tick(60_000);
  await settle();
  await assert.rejects(Promise.race([stopping, Promise.resolve('no stop failure reported')]), /host.*stop/);
});

test('a child process error does not stand in for its exit event', async (t) => {
  const { host, process } = start(t);
  t.mock.method(console, 'error', () => {});
  let stopped = false;
  const stopping = host.stop(60_000).then(() => { stopped = true; });
  process.emit('error', new Error('kill denied'));
  await settle();
  assert.equal(stopped, false);
  process.emit('exit', 0);
  await stopping;
});

test('Quit refuses new calls once engine input has closed', async (t) => {
  const { host, process } = start(t);
  const stopping = host.stop(patience);
  await assert.rejects(Promise.race([host.call('send', { text: 'too late' }), Promise.resolve('call was accepted')]), /error.host.gone/);
  process.emit('exit', 0);
  await stopping;
});

test('a host that could not spawn can still be quit without waiting for a nonexistent exit', async (t) => {
  const { host, process } = start(t);
  t.mock.method(console, 'error', () => {});
  delete process.pid;
  process.emit('error', new Error('spawn failed'));
  await host.stop(patience);
  assert.equal(process.killed, false);
  await assert.rejects(host.call('state', {}), /error.host.gone/);
});

test('a denied kill can be retried and still requires confirmed exit', async (t) => {
  const { host, process } = start(t);
  let kills = 0;
  process.kill = () => ++kills > 1;
  const failed = assert.rejects(host.stop(patience), /error.host.stop/);
  t.mock.timers.tick(patience);
  await failed;
  let stopped = false;
  const retry = host.stop(patience).then(() => { stopped = true; });
  t.mock.timers.tick(patience);
  await settle();
  assert.equal(kills, 2);
  assert.equal(stopped, false);
  process.emit('exit', 0);
  await retry;
  assert.equal(stopped, true);
});

test('a runtime process error keeps pending calls and does not schedule a replacement', async (t) => {
  const { host, process, ended } = start(t);
  t.mock.method(console, 'error', () => {});
  const waiting = host.call('state', {});
  void waiting.catch(() => {});
  process.emit('error', new Error('runtime failure'));
  t.mock.timers.tick(30_000);
  await settle();
  process.stdout.write(JSON.stringify({ id: 1, ok: { kept: true } }) + '\n');
  try {
    assert.deepEqual(ended, [], 'a runtime error must not tell main.ts to replace a live host');
    assert.deepEqual(await waiting, { kept: true });
  } finally {
    const stopping = host.stop(patience);
    process.emit('exit', 0);
    await stopping;
  }
});
