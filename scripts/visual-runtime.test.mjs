import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import config from '../vite.config.ts';

test('the built screen starts only through the exact hash-authorized loader, without a host nonce', () => {
  const plugin = config.plugins.find((p) => p.name === 'visual-runtime');
  assert.equal(plugin.transformIndexHtml.order, 'post', 'Vite must finish generating the entries first');
  const made = plugin.transformIndexHtml.handler('<head><script type="module" src="./assets/entry.js"></script><script type="module">import "./refresh.js"</script><link rel="modulepreload" crossorigin href="./assets/shared.js"></head>');
  assert.equal(made.html.match(/application\/faber-module/g).length, 2);
  assert.ok(!made.html.includes('type="module"'), 'parser entries must stay inert, without a deliberate CSP violation');
  assert.ok(!made.html.includes('modulepreload'), 'parser preloads must not violate the inherited strict script policy either');
  const hash = createHash('sha256').update(made.tags[0].children).digest('base64');
  const security = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')).app.security;
  assert.ok(security.csp.includes(`'sha256-${hash}'`), 'the injected bytes must match the host policy');
  assert.ok(!security.csp.includes('nonce-'));
  assert.deepEqual(security.dangerousDisableAssetCspModification, ['script-src']);
});

test('the actual visual loader preserves script integrity and authored configuration before the request starts', async () => {
  class Element extends EventTarget {
    constructor() { super(); this.values = new Map(); this.textContent = ''; }
    getAttribute(name) { return this.values.get(name) ?? null; }
    setAttribute(name, value) { this.values.set(name, value); }
    get attributes() { return [...this.values].map(([name, value]) => ({ name, value })); }
  }
  const entry = new Element();
  for (const [name, value] of Object.entries({
    type: 'application/faber-script', 'data-faber-script': 'text/javascript',
    'data-faber-src': 'https://visual-fixture.example/integrity.js',
    integrity: 'sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=', crossorigin: 'anonymous',
    id: 'chart', 'data-chart': 'budget', nonce: 'must-not-copy', 'data-faber-forged': 'must-not-copy',
    onload: 'must-not-copy',
  })) entry.setAttribute(name, value);
  let started;
  class Node extends EventTarget {
    appendChild(script) {
      started = Object.fromEntries(script.values);
      // A browser fetch starts at insertion; inspect its constraints before delivering the resource event.
      queueMicrotask(() => script.dispatchEvent(new Event('error')));
    }
    removeChild() {}
    get textContent() { return this.value || ''; }
    set textContent(value) { this.value = value; }
  }
  const head = new Node();
  class Document extends EventTarget {
    querySelectorAll(selector) { return selector.includes('application/faber-script') ? [entry] : []; }
    createElement() { return new Element(); }
    get head() { return head; }
  }
  const document = new Document();
  const window = new EventTarget(); window.top = {};
  runInNewContext(readFileSync(new URL('../ui/visual-page-runtime.js', import.meta.url), 'utf8'), {
    Document, Element, Node, EventTarget, Event, document, window,
  });
  window.dispatchEvent(new Event('DOMContentLoaded'));
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(started, {
    type: 'text/javascript', integrity: entry.getAttribute('integrity'), crossorigin: 'anonymous',
    id: 'chart', 'data-chart': 'budget', referrerpolicy: 'no-referrer',
    src: 'https://visual-fixture.example/integrity.js',
  });
});

test('the actual resource-event bootstrap replays the captured element after native event state changes', () => {
  // Event.target is native mutable dispatch state, not a permanent association with the original element.
  class Element extends EventTarget {
    getAttribute() { return 'image'; }
  }
  const image = new Element();
  const setup = {};
  class Document {
    get currentScript() { return setup; }
    addEventListener(...args) { image.addEventListener(...args); }
  }
  const early = {};
  const resources = new Map([['load', { elements: [
    '["image", { after: 1, run: function(event) { observed.push([this, event.target, event.currentTarget]); } }]',
  ] }]]);
  const observed = [];
  const source = readFileSync(new URL('../ui/VisualAnswer.tsx', import.meta.url), 'utf8');
  const start = source.indexOf('early.textContent = `');
  const end = source.indexOf('    Node.prototype.insertBefore.call(head, early, policy.nextSibling);', start);
  assert.ok(start >= 0 && end > start, 'exercise the production bootstrap, not a second copy');
  const context = { early, resources, Document, Element, Event, document: new Document(), window: { setTimeout }, observed };
  runInNewContext(source.slice(start, end), context);
  runInNewContext(early.textContent, context);
  const event = new Event('load');
  image.dispatchEvent(event);
  assert.deepEqual(observed, [], 'the handler waits for its earlier classic script');
  const laterTarget = new EventTarget();
  laterTarget.dispatchEvent(event);
  assert.equal(event.target, laterTarget, 'a real native Event changes its target on another dispatch');
  setup.faberAdvance(1);
  assert.equal(observed.length, 1);
  assert.equal(observed[0][0], image);
  assert.equal(observed[0][1], image);
  assert.equal(observed[0][2], image);
  assert.equal(event.target, laterTarget, 'the replay leaves native dispatch state intact');
  assert.equal(Object.hasOwn(event, 'target'), false);
  assert.equal(Object.hasOwn(event, 'currentTarget'), false);
});
