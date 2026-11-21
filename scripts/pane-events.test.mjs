import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { EventEmitter } from 'node:events';
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

class Contents extends EventEmitter {
  debugger = Object.assign(new EventEmitter(), {
    attach() {}, detach() {}, isAttached: () => true,
    sendCommand: async (method) => {
      if (method === 'Page.enable') assert.equal(this.started, true, 'the renderer starts before DevTools waits for it');
      return {};
    },
  });
  started = false;
  url = 'about:blank';
  loading = false;
  setWindowOpenHandler() {}
  setUserAgent() {}
  getUserAgent() { return 'test'; }
  getURL() { return this.url; }
  isLoading() { return this.loading; }
  isDestroyed() { return false; }
  async loadURL(url) { this.started = true; this.url = url; }
  close() {}
  stop() {}
}
class View { webContents = new Contents(); constructor(options) { this.session = options.webPreferences.session; } setBounds() {} }
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

test('the blank document starts before opening the DevTools session', async () => {
  let finish;
  const answer = new Promise((resolve) => { finish = resolve; });
  const window = { isDestroyed: () => false, contentView: {
    addChildView(view) {
      const contents = view.webContents;
      let ready = false;
      contents.loadURL = async (url) => {
        assert.equal(url, 'about:blank');
        ready = true;
      };
      contents.debugger.sendCommand = async () => {
        assert.ok(ready, 'DevTools needs the first document');
      };
    },
    removeChildView() {},
  } };
  const pane = new Pane({ window: () => window, note: (note) => { if (note.id === 1) finish(note); } });
  pane.order({ kind: 'pane', op: 'open', view: 'main', profile: 'test', id: 1 });
  assert.deepEqual(await answer, { kind: 'paneAnswer', id: 1, ok: null });
  pane.closeAll();
});

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

test('developer orders isolate profiles and restore approved extensions before navigation', async (t) => {
  const parent = await realpath(await mkdtemp(path.resolve('node_modules', '.pane-dev-test-')));
  t.after(() => rm(parent, { recursive: true, force: true }));
  const root = path.join(parent, 'profiles');
  const folder = path.join(parent, 'unpacked');
  await mkdir(root);
  await mkdir(folder);
  await writeFile(path.join(folder, 'manifest.json'), '{"name":"Test","version":"1","manifest_version":3}');

  let window = null;
  let id = 0;
  const replies = new Map();
  const operations = [];
  const disk = [];
  const extensionId = 'a'.repeat(32);
  const previous = session.fromPath;
  t.after(() => { session.fromPath = previous; });
  session.fromPath = (location, options) => {
    assert.ok(location.startsWith(root + path.sep));
    assert.deepEqual(options, { cache: false });
    const held = session.fromPartition();
    disk.push(held);
    const loaded = new Map();
    held.webRequest.onBeforeRequest = (listener) => {
      held.permits = (url) => {
        let allowed;
        listener({ url }, ({ cancel }) => { allowed = !cancel; });
        return allowed;
      };
    };
    held.clearData = async () => { operations.push('wipe'); };
    held.extensions = {
      getAllExtensions: () => [...loaded.values()],
      getExtension: (id) => loaded.get(id),
      async loadExtension(snapshot, options) {
        assert.notEqual(snapshot, folder);
        assert.deepEqual(options, { allowFileAccess: false });
        await readFile(path.join(snapshot, 'manifest.json'));
        const extension = { id: extensionId, path: snapshot, name: 'Test', version: '1' };
        loaded.set(extension.id, extension);
        operations.push('install');
        return extension;
      },
      removeExtension(id) { operations.push('unload'); loaded.delete(id); },
    };
    return held;
  };

  const pane = new Pane({ window: () => window, extensionStorage: () => root, note: (note) => replies.get(note.id)?.(note) });
  function order(op, view = 'main') {
    return new Promise((resolve) => {
      const number = ++id;
      replies.set(number, (note) => { replies.delete(number); resolve(note); });
      pane.order({ kind: 'pane', id: number, view, ...op });
    });
  }
  const call = (method, params) => order({ op: 'call', method, params });
  const mode = (enabled) => call('Faber.extensionMode', { enabled });
  const extensions = (scope, action) => call('Faber.extensions', { scope, action, folder });
  const open = (view, development) => order({ op: 'open', profile: 'profile', development }, view);

  assert.match((await extensions('one', 'list')).err, /mode is off/);
  assert.equal((await mode(true)).err, undefined);
  assert.deepEqual((await extensions('one', 'load')).ok, [{ name: 'Test', version: '1', folder }]);
  assert.equal(disk.length, 0);
  assert.match((await open('main', 'one')).err, /closed/);

  const views = new Set();
  window = { isDestroyed: () => false, contentView: {
    addChildView(view) {
      views.add(view);
      const load = view.webContents.loadURL.bind(view.webContents);
      view.webContents.loadURL = async (url) => { operations.push(url); await load(url); };
    },
    removeChildView(view) { views.delete(view); },
  } };
  operations.length = 0;
  const opening = open('main', 'one');
  const navigation = order({ op: 'load', url: 'https://example.org/' });
  assert.equal((await opening).err, undefined);
  assert.equal((await navigation).err, undefined);
  assert.ok(operations.lastIndexOf('install') < operations.indexOf('about:blank'));
  assert.deepEqual(operations.slice(-2), ['about:blank', 'https://example.org/']);
  const profile = [...views][0].session;
  assert.ok(disk.includes(profile));
  assert.equal((await open('behind', 'one')).err, undefined);
  assert.equal([...views][1].session, profile);
  assert.equal(profile.permits('chrome-extension://' + extensionId + '/page.html'), true);
  assert.equal(profile.permits('chrome-extension://' + 'b'.repeat(32) + '/page.html'), false);
  assert.equal(profile.permits('faber://app/'), false);
  assert.deepEqual((await extensions('two', 'list')).ok, []);
  const installs = operations.filter((operation) => operation === 'install').length;
  assert.equal((await extensions('two', 'load')).err, undefined);
  assert.equal(operations.filter((operation) => operation === 'install').length, installs);
  assert.equal((await extensions('two', 'remove')).err, undefined);
  assert.equal(profile.extensions.getAllExtensions().length, 1);
  assert.match((await call('Network.setCookies', { cookies: [] })).err, /cannot be imported/);

  const disabling = mode(false);
  assert.equal(views.size, 0);
  assert.equal(profile.permits('https://example.org/'), false);
  assert.equal((await disabling).err, undefined);
  assert.equal(profile.extensions.getAllExtensions().length, 0);
  assert.deepEqual(operations.slice(-2), ['unload', 'wipe']);
  await mode(true);
  assert.deepEqual((await extensions('one', 'list')).ok, [{ name: 'Test', version: '1', folder }]);
  const count = disk.length;
  assert.equal((await open('main', 'two')).err, undefined);
  assert.equal(disk.length, count);
  assert.equal(profile.permits('chrome-extension://' + extensionId + '/page.html'), false);
  assert.equal((await open('behind')).err, undefined);
  assert.ok(!disk.includes([...views].at(-1).session));
  pane.closeAll();
  await mode(false);
});
