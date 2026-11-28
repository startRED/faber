// Run with Electron after electron:build, passing an empty scratch directory.
import assert from 'node:assert/strict';
import { realpathSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import path from 'node:path';
import { app, BrowserWindow } from 'electron';
import { Pane } from '../out/electron/pane.js';
import { resetExtensionStorage } from '../out/electron/pane-extensions.js';

const root = realpathSync(process.argv[2]);
app.setPath('userData', root);
app.disableHardwareAcceleration();
app.on('window-all-closed', () => {});
let stage = 'startup';
const deadline = setTimeout(() => { console.error(`Browser extension proof timed out: ${stage}`); app.exit(1); }, 25_000);
void app.whenReady().then(run).catch((error) => {
  console.error(error);
  app.exit(1);
});

async function run() {
  const storage = await resetExtensionStorage(path.join(root, 'profiles'));
  const folder = path.join(root, 'unpacked');
  await mkdir(folder);
  const write = async (version) => {
    await writeFile(path.join(folder, 'manifest.json'), JSON.stringify({
      name: 'Pane proof',
      version,
      manifest_version: 3,
      content_scripts: [{ matches: ['http://127.0.0.1/*'], js: ['content.js'] }],
    }));
    await writeFile(path.join(folder, 'content.js'), `document.body.dataset.extension = ${JSON.stringify(version)};`);
  };
  await write('1.0');
  const server = createServer((_request, response) => {
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end('<!doctype html><title>Browser extension proof</title><p>Local test page</p>');
  });
  await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}/`;
  const window = new BrowserWindow({ show: false });
  const answers = new Map();
  let finished;
  const pane = new Pane({
    window: () => window,
    extensionStorage: () => storage,
    extensionRegistry: path.join(root, 'registry.json'),
    note: (note) => {
      if (note.kind === 'paneAnswer') {
        const { resolve, reject } = answers.get(note.id);
        answers.delete(note.id);
        if (note.err) reject(new Error(note.err)); else resolve(note.ok);
      }
      if (note.kind === 'paneEvent' && note.event.kind === 'finished') finished?.();
    },
  });
  let counter = 0;
  const ask = (order) => new Promise((resolve, reject) => {
    const id = ++counter;
    answers.set(id, { resolve, reject });
    pane.order({ kind: 'pane', view: 'main', id, ...order });
  });
  const call = (method, params) => ask({ op: 'call', method, params });
  const mode = (enabled) => call('Faber.extensionMode', { enabled });
  const extensions = (action, source, scope = 'proof') => call('Faber.extensions', { scope, action, folder: source });
  const visit = async () => {
    const loaded = new Promise((resolve) => { finished = resolve; });
    await ask({ op: 'load', url });
    await loaded;
    finished = undefined;
    const value = await call('Runtime.evaluate', { expression: 'document.body.dataset.extension ?? ""', returnByValue: true });
    return value.result.value;
  };
  try {
    await assert.rejects(extensions('load', folder), /mode is off/);
    await mode(true);
    await assert.rejects(extensions('reload', folder), /not registered/);
    await extensions('load', folder);
    assert.deepEqual(await extensions('list', undefined, 'another'), []);
    await assert.rejects(extensions('reload', folder, 'another'), /not registered/);
    stage = 'opening developer view';
    await ask({ op: 'open', profile: 'faber-proof', development: 'proof' });
    stage = 'navigating to test page';
    assert.equal(await visit(), '1.0');
    await write('2.0');
    assert.equal((await extensions('reload', folder))[0].version, '2.0');
    assert.equal(await visit(), '2.0');
    assert.deepEqual(await extensions('remove', folder), []);
    assert.equal(await visit(), '');
    await mode(false);
    await assert.rejects(extensions('list'), /mode is off/);
    console.log('Browser pane: off by default, isolated scopes, loaded, reloaded and removed on a real local page.');
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  } finally {
    clearTimeout(deadline);
    pane.closeAll();
    window.destroy();
    server.close();
    app.exit(process.exitCode ?? 0);
  }
}
