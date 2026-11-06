import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import test from 'node:test';
import vm from 'node:vm';

test('a renderer refusing unload hides to the tray without being destroyed', () => {
  const main = readFileSync(new URL('./main.ts', import.meta.url), 'utf8');
  const source = main.slice(main.indexOf('function createWindow()'), main.indexOf('\n/**', main.indexOf('function createWindow()'))).replaceAll('import.meta.dirname', "'.'");
  let hidden = false;
  const contents = Object.assign(new EventEmitter(), {
    setWebRTCIPHandlingPolicy() {}, setWindowOpenHandler() {}, send() {}, session: { flushStorageData() {} },
  });
  const window = Object.assign(new EventEmitter(), {
    webContents: contents, loadURL() {}, hide() { hidden = true; },
  });
  const create = vm.runInNewContext(stripTypeScriptTypes(`${source}\ncreateWindow`), {
    BrowserWindow: function () { return window; }, process, path: { join: () => 'preload.cjs' },
    FRAME: '#101012', ORIGIN: 'faber://app', testmode: { testing: false, placement: {}, startPage: () => '' },
    guardVisualFrames() {}, pane: { watch() {} }, startup: { windowBuilt() {} },
    setTimeout() {}, clearTimeout() {}, REVEAL_FAILSAFE_MS: 3000,
    closingForQuit: false,
  });
  create();
  let overrodeRefusal = false;
  contents.emit('will-prevent-unload', { preventDefault() { overrodeRefusal = true; } });
  assert.equal(hidden, true, 'the refused close must remove the window from the screen');
  assert.equal(overrodeRefusal, false, 'overriding beforeunload would destroy the only draft copy');
});

test('Quit preserves the engine and tray until a draft-protected window actually closes', async () => {
  const main = readFileSync(new URL('./main.ts', import.meta.url), 'utf8');
  const source = main.slice(main.indexOf('let stopping = false;'), main.indexOf('void app.whenReady()'));
  const app = new EventEmitter();
  app.quit = () => {};
  let stops = 0;
  let released = 0;
  let closed = 0;
  const window = { close() { closed++; } };
  vm.runInNewContext(stripTypeScriptTypes(source), {
    app, win: window, host: { stop() { stops++; return Promise.resolve(); } },
    HOST_STOP_MS: 60_000, release() { released++; }, pane: { closeAll() {} },
  });
  app.emit('before-quit', { preventDefault() {} });
  await Promise.resolve();
  assert.equal(stops, 0, 'a pending or refused renderer close must not stop the engine');
  assert.equal(released, 0, 'the tray must stay usable after a refused Quit');
  assert.equal(closed, 1, 'Quit first asks the window to save or refuse its close');
});
