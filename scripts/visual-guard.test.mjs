import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('../src-tauri/src/visual.js', import.meta.url), 'utf8');
const section = (from, to) => {
  const start = source.indexOf(from), end = source.indexOf(to, start);
  assert.ok(start >= 0 && end > start, `exercise the production guard: ${from}`);
  return source.slice(start, end);
};
const helpers = section('  const lockMethod =', '  // Only stylesheets');
const parts = {
  frame: section('  for (const constructor of [window.HTMLIFrameElement', '  const refuseOpen'),
  open: section('  const refuseOpen', '  const internals ='),
  shadow: section('  const internals =', "  lockMethod(Element.prototype, 'attachShadow'"),
  rtc: section('  // These transports', '  // A connected stylesheet'),
};

function page() {
  // The empty child deliberately has no initialization script, as with WebKit's initial about:blank document.
  const child = { document: { URL: 'about:blank' } };
  class Frame {
    get contentWindow() { return child; }
    get contentDocument() { return child.document; }
  }
  class IFrame {
    get contentWindow() { return child; }
    get contentDocument() { return child.document; }
  }
  const root = { children: [new IFrame()] };
  class Internals { get shadowRoot() { return root; } }
  class Window { open() { return child; } }
  const window = new Window();
  Object.assign(window, { HTMLIFrameElement: IFrame, HTMLFrameElement: Frame, ElementInternals: Internals });
  const transports = [];
  for (const name of ['RTCPeerConnection', 'webkitRTCPeerConnection', 'RTCDataChannel', 'webkitRTCDataChannel']) {
    window[name] = class { constructor() { throw Error('a transport escaped'); } };
    transports.push(window[name]);
  }
  let watched = 0;
  const context = {
    window, Window, descriptor: Object.getOwnPropertyDescriptor, define: Object.defineProperty, apply: Reflect.apply,
    watchShadow: () => watched++, clean: (root) => { root.children = []; },
  };
  const mutant = process.env.FABER_VISUAL_GUARD_MUTANT;
  const code = Object.entries(parts).filter(([name]) => name !== mutant || name === 'rtc').map(([name, code]) =>
    name === 'rtc' && mutant === 'rtc' ? code.replace(", 'RTCDataChannel', 'webkitRTCDataChannel'", '') : code).join('\n');
  runInNewContext(helpers + code, context);
  return { window, Frame, IFrame, Internals, root, child, transports, watched: () => watched };
}

test('visual guard: empty iframe and frame realms cannot be read through either native accessor', () => {
  const { Frame, IFrame } = page();
  for (const Constructor of [Frame, IFrame]) {
    const frame = new Constructor();
    for (const name of ['contentWindow', 'contentDocument']) {
      const entry = Object.getOwnPropertyDescriptor(Constructor.prototype, name);
      assert.equal(frame[name], null);
      assert.equal(entry.get.call(frame), null, 'borrowing the accessor cannot recover its native getter');
      assert.equal(entry.configurable, false);
    }
  }
});

test('visual guard: opening an existing named empty child cannot recover its realm', () => {
  const { window } = page();
  assert.equal(window.open('', 'hidden-child'), null);
  assert.equal(Object.getPrototypeOf(window).open.call(window, '', 'hidden-child'), null);
  assert.equal(Object.getOwnPropertyDescriptor(window, 'open').writable, false);
  assert.equal(Object.getOwnPropertyDescriptor(window, 'open').configurable, false);
});

test('visual guard: ElementInternals cleans a closed root before exposing it', () => {
  const { Internals, root, watched } = page();
  assert.equal(root.children.length, 1, 'the closed root really contains a previously created frame');
  assert.equal(new Internals().shadowRoot, root);
  assert.equal(root.children.length, 0);
  assert.equal(watched(), 1);
  assert.equal(Object.getOwnPropertyDescriptor(Internals.prototype, 'shadowRoot').configurable, false);
});

test('visual guard: WebRTC peer and data-channel globals and prototype constructors stay locked', () => {
  const { window, transports } = page();
  for (const name of ['RTCPeerConnection', 'webkitRTCPeerConnection', 'RTCDataChannel', 'webkitRTCDataChannel']) {
    assert.equal(window[name], undefined);
    const entry = Object.getOwnPropertyDescriptor(window, name);
    assert.equal(entry.configurable, false);
    assert.equal(entry.writable, false);
  }
  for (const transport of transports) assert.equal(transport.prototype.constructor, undefined);
});

test('visual guard: the parser drain removes indexed and named children before authored replay', () => {
  const child = { document: { URL: 'about:blank' } };
  const document = { children: [child] };
  const window = new EventTarget();
  Object.defineProperties(window, {
    0: { get: () => document.children[0] },
    frames: { get: () => window },
    parserChild: { get: () => document.children[0] },
  });
  assert.equal(window[0], child, 'the simulated parser has already created a real child reference');
  const pending = [{ child }];
  let disconnected = false;
  const context = {
    window, document, parsing: true, parserGuard: {}, apply: Reflect.apply,
    nativeObserve() {}, nativeAddListener: EventTarget.prototype.addEventListener,
    nativeTakeRecords: () => pending,
    cleanParsed(records) { assert.equal(records, pending); },
    clean(node) { assert.equal(node, document); document.children = []; },
    nativeDisconnect() { disconnected = true; },
  };
  if (process.env.FABER_VISUAL_GUARD_MUTANT !== 'parser') {
    runInNewContext(section('  apply(nativeObserve, parserGuard, [document,', '  const cleanArgument ='), context);
  }
  let replayed;
  window.addEventListener('DOMContentLoaded', () => {
    replayed = [window[0], window.frames[0], window.parserChild];
  });
  window.dispatchEvent(new Event('DOMContentLoaded'));
  assert.deepEqual(replayed, [undefined, undefined, undefined]);
  assert.equal(disconnected, true, 'the observer stops once the real parser has drained');
});
