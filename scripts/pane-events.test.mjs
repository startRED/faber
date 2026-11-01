import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { EventEmitter } from 'node:events';

class Contents extends EventEmitter {
  debugger = Object.assign(new EventEmitter(), { attach() {}, detach() {}, isAttached: () => true, sendCommand: async () => ({}) });
  url = 'about:blank';
  loading = false;
  setWindowOpenHandler() {}
  setUserAgent() {}
  getUserAgent() { return 'test'; }
  getURL() { return this.url; }
  isLoading() { return this.loading; }
  isDestroyed() { return false; }
  async loadURL(url) { this.url = url; }
  close() {}
  stop() {}
}
class View { webContents = new Contents(); setBounds() {} }
const sessions = [];
const session = { fromPartition() {
  const held = Object.assign(new EventEmitter(), {
    setPermissionRequestHandler() {}, setPermissionCheckHandler() {}, setDevicePermissionHandler() {}, setSpellCheckerEnabled() {},
    webRequest: { onBeforeRequest() {} }, clearData: async () => {}, clearAuthCache: async () => {},
    clearHostResolverCache: async () => {}, closeAllConnections: async () => {},
  });
  sessions.push(held);
  return held;
} };
globalThis.paneTestElectron = { WebContentsView: View, session };
registerHooks({ resolve(specifier, context, next) {
  if (specifier === 'electron') return { url: 'test:electron', shortCircuit: true };
  return next(specifier, context);
}, load(url, context, next) {
  if (url === 'test:electron') return { format: 'module', shortCircuit: true, source: 'export const { WebContentsView, session } = globalThis.paneTestElectron;' };
  return next(url, context);
} });
const { Pane } = await import('../electron/pane.ts');

test('overlapping and cancelled navigations finish only when loading stops', async () => {
  const notes = [];
  let view;
  const window = { isDestroyed: () => false, contentView: { addChildView(value) { view = value; }, removeChildView() {} } };
  const pane = new Pane({ window: () => window, note: (note) => notes.push(note) });
  pane.order({ kind: 'pane', op: 'open', view: 'main', profile: 'test', id: 1 });
  while (!notes.some((note) => note.id === 1)) await new Promise((resolve) => setImmediate(resolve));
  const contents = view.webContents;
  const address = 'https://example.org/';
  contents.url = address;
  contents.loading = true;
  contents.emit('did-start-navigation', { isMainFrame: true, isSameDocument: false, url: address });
  contents.emit('did-start-navigation', { isMainFrame: true, isSameDocument: false, url: address });
  contents.emit('did-fail-provisional-load', {}, -3, 'aborted', address, true);
  contents.emit('did-stop-loading');
  assert.equal(notes.filter((note) => note.event?.kind === 'finished').length, 0);
  contents.loading = false;
  contents.emit('did-stop-loading');
  contents.emit('did-stop-loading');
  assert.deepEqual(notes.filter((note) => note.event).map((note) => [note.event.kind, note.event.navigation]), [['started', 1], ['started', 2], ['finished', 2]]);
  pane.closeAll();
});

test('a failed profile wipe is never recycled', async () => {
  const notes = [];
  const window = { isDestroyed: () => false, contentView: { addChildView() {}, removeChildView() {} } };
  const pane = new Pane({ window: () => window, note: (note) => notes.push(note) });
  const open = async (profile, id) => {
    pane.order({ kind: 'pane', op: 'open', view: 'main', profile, id });
    while (!notes.some((note) => note.id === id)) await new Promise((resolve) => setImmediate(resolve));
    assert.equal(notes.find((note) => note.id === id).err, undefined);
  };
  await open('first', 1);
  const dirty = sessions.at(-1);
  dirty.clearData = async () => { throw new Error('wipe failed'); };
  pane.closeAll();
  await new Promise((resolve) => setImmediate(resolve));
  await open('second', 2);
  assert.notEqual(sessions.at(-1), dirty);
  pane.closeAll();
});
