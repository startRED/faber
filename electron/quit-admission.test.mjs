import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { registerHooks } from 'node:module';
import { PassThrough, Writable } from 'node:stream';
import test from 'node:test';

process.env.FABER_SIDECAR = 'fake-sidecar';
registerHooks({
  resolve(specifier, context, next) {
    if (['electron', 'node:child_process', './orders.ts'].includes(specifier)) {
      return { url: `quit-test:${specifier}`, shortCircuit: true };
    }
    return next(specifier, context);
  },
  load(url, context, next) {
    const sources = {
      'quit-test:electron': "export const app = { isPackaged: false, getVersion: () => 'test' };",
      'quit-test:node:child_process': 'export const spawn = () => globalThis.quitTestChild;',
      'quit-test:./orders.ts': "export const LOGIN_ITEM = 'Faber';",
    };
    return url in sources ? { format: 'module', source: sources[url], shortCircuit: true } : next(url, context);
  },
});
const { Sidecar } = await import('./sidecar.ts');

test('quit refuses a new send after closing engine input', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const requests = [];
  const child = Object.assign(new EventEmitter(), {
    stdout: new PassThrough(),
    stdin: new Writable({ write(line, _encoding, done) { requests.push(JSON.parse(line)); done(); } }),
    kill() { assert.fail('this engine exits without forced termination'); },
  });
  globalThis.quitTestChild = child;
  const host = Sidecar.start({ event() {}, order() {}, ended() {} });
  const stopping = host.stop(60_000);
  try {
    assert.equal(child.stdin.writableEnded, true, 'shutdown must request the final save through EOF');
    await assert.rejects(
      Promise.race([host.call('send', { text: 'arrived during Quit' }), Promise.resolve('request was not refused')]),
      /error.host.gone/,
    );
    assert.deepEqual(requests, [], 'a refused send must never reach the engine');
  } finally {
    child.emit('exit', 0);
    await stopping;
    child.stdout.end();
    delete globalThis.quitTestChild;
  }
});
