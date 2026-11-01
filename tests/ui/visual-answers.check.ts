import { createComponent, createSignal, ErrorBoundary, Show } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppState, type Conversation, type Message, type VisualAnswer } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { applyLook } from '../../ui/appearance.ts';
import { Thread } from '../../ui/Thread.tsx';
import { openedVisual, showVisual, VisualFiles, VisualPane, VisualReply, visualDocument, VISUAL_CSP, VISUAL_LIVE_CSP } from '../../ui/VisualAnswer.tsx';
import { MERMAID_INTEGRITY, MERMAID_URL, mermaidDocument } from '../../ui/visual-mermaid.ts';
import { openedSite, showSite } from '../../ui/Site.tsx';
import { openedFile, showFile } from '../../ui/fileref.ts';
import { store } from '../../ui/running.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { menu, openMenu, closeMenu } from '../../ui/contextmenu.ts';
import { check as registerCheck, deepEqual, equal, ok } from './check.ts';

// Every frame check uses the real single-use protocol, with an explicit saved permission.
const prepared = new Map<string, { document: string; live: boolean }>();
function check(name: string, run: () => void | Promise<void>) {
  registerCheck(name, async () => {
    const web = (await api.state()).settings.webSearch ?? 'live';
    await api.saveSettings({ webSearch: 'off' });
    const saved = api.visualPage;
    api.visualPage = async (id, revision, document) => {
      const page = await saved(id, revision, document);
      prepared.set(page.url, { document, live: page.live });
      return page;
    };
    try { await run(); } finally {
      api.visualPage = saved; prepared.clear();
      await api.saveSettings({ webSearch: web });
    }
  });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
function loadedVisual(host: HTMLElement) {
  const probe = crypto.randomUUID();
  let url: string | undefined;
  // What reached the screen, for a failure message: reports from any frame, and the frames' own load events.
  const seen = { reports: 0, otherSource: 0, otherOrigin: '', loads: 0, inserted: 0 };
  const moves = new MutationObserver((records) => {
    for (const r of records) for (const n of r.addedNodes) if (n instanceof Element && (n.localName === 'iframe' || n.querySelector('iframe'))) seen.inserted++;
  });
  moves.observe(host, { childList: true, subtree: true });
  const listen = (e: MessageEvent) => {
    if (e.data?.probe !== probe) return;
    seen.reports++;
    if (e.origin !== 'null') seen.otherOrigin = e.origin;
    else if (e.source !== host.querySelector('iframe')?.contentWindow) seen.otherSource++;
    else if (typeof e.data.url === 'string') url = e.data.url;
  };
  const load = (e: Event) => { if (e.target instanceof HTMLIFrameElement) seen.loads++; };
  window.addEventListener('message', listen);
  host.addEventListener('load', load, true);
  const report = `document.addEventListener('DOMContentLoaded',()=>parent.postMessage({probe:${JSON.stringify(probe)},url:location.href},'*'));`;
  return {
    source: (text: string) => text + `<script>${report}</script>`,
    prepared: (text: string) => {
      const script = `<script type="application/faber-script" data-faber-script="text/javascript">${report}</script>`;
      const end = text.lastIndexOf('</body>');
      return end < 0 ? text + script : text.slice(0, end) + script + text.slice(end);
    },
    url: () => url,
    why: () => `reports ${seen.reports}, from another frame ${seen.otherSource}, other origin ${seen.otherOrigin || 'none'}, frame loads ${seen.loads}, frame insertions ${seen.inserted}, frame src ${host.querySelector('iframe')?.src ?? 'none'}, the pane asked it to open ${host.querySelector('iframe')?.dataset.asked ?? 'nothing'}`,
    stop: () => { window.removeEventListener('message', listen); host.removeEventListener('load', load, true); moves.disconnect(); },
  };
}
async function until(test: () => unknown, what: string | (() => string) = 'the visual answer appeared before the deadline') {
  const end = performance.now() + 15000;
  while (!test() && performance.now() < end) await settle();
  ok(test(), typeof what === 'string' ? what : what());
}

// Wakes on what can change `test`: a message a frame posts, a frame's load, a change of the DOM. The 15 s is only the guard;
// on failure it names the wait and what the frame did, so a failure on a loaded runner says which stage it was.
function settled(host: HTMLElement, test: () => unknown, what: () => string) {
  return new Promise<void>((resolve, reject) => {
    const moves = new MutationObserver(() => wake());
    const stop = () => { clearTimeout(guard); window.removeEventListener('message', wake); host.removeEventListener('load', wake, true); moves.disconnect(); };
    const wake = () => { if (test()) { stop(); resolve(); } };
    const guard = setTimeout(() => { stop(); try { ok(test(), what()); resolve(); } catch (e) { reject(e); } }, 15000);
    window.addEventListener('message', wake);
    host.addEventListener('load', wake, true);
    moves.observe(host, { childList: true, subtree: true, attributes: true });
    wake();
  });
}

check('visual answers: live policy keeps the opaque sandbox, first policy and inert external scripts without a host nonce', () => {
  const source = '<form name="documentElement"><script nonce="host" src="https://cdn.example.com/chart.js"></script></form><script src="http://example.com/plain.js"></script><script>window.ready=true</script>';
  for (const live of [false, true]) {
    const doc = new DOMParser().parseFromString(visualDocument(source, live), 'text/html');
    equal(doc.head.firstElementChild!.getAttribute('content'), live ? VISUAL_LIVE_CSP : VISUAL_CSP);
    equal(doc.querySelector('[nonce]'), null);
    equal(doc.querySelector('script[src]'), null, 'only the fixed loader can start an external script');
    equal(doc.querySelectorAll('[data-faber-src]').length, live ? 1 : 0);
    if (live) equal(doc.querySelector('[data-faber-src]')!.getAttribute('type'), 'application/faber-script');
    ok(!doc.documentElement.outerHTML.includes('plain.js'), 'plain HTTP is never replayed');
    for (const directive of ["base-uri 'none'", "form-action 'none'", "frame-src 'none'", "object-src 'none'"]) {
      ok(doc.head.firstElementChild!.getAttribute('content')!.includes(directive));
    }
  }
});

check('visual answers: web changes apply on the next opened version, keep one frame and do not reload an open page', async () => {
  const host = document.createElement('div'); document.body.append(host);
  let calls = 0;
  let answered = 0;
  const saved = api.visualPage;
  api.visualPage = async (id, revision, document) => { calls++; const page = await saved(id, revision, document); answered++; return page; };
  const a = answer();
  const loaded = loadedVisual(host);
  a.versions = a.versions.map((v) => ({ ...v, source: loaded.source(v.source) }));
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'policy', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    const seen = () => `${host.querySelector('.visual-version')?.textContent}, previous ${host.querySelector<HTMLButtonElement>('.visual-prev')?.disabled ? 'disabled' : 'enabled'}, pages asked ${calls} and answered ${answered}, frame src ${host.querySelector('iframe')?.src ?? 'none'}, loaded url ${loaded.url() ?? 'none'}, its page live ${prepared.get(loaded.url() ?? '')?.live}; ${loaded.why()}`;
    await settled(host, () => host.querySelector('iframe'), () => `the first frame of the opened version did not appear: ${seen()}`);
    const frame = host.querySelector('iframe')!;
    const offline = frame.src;
    equal(frame.getAttribute('sandbox'), 'allow-scripts');
    equal(prepared.get(offline)!.live, false);
    await api.saveSettings({ webSearch: 'live' });
    const live = await api.visualPolicy();
    await settle();
    equal(frame.src, offline, 'saving the setting leaves current page state intact');
    equal(calls, 1);
    const before = loaded.url() ? 'had' : 'had not';
    host.querySelector<HTMLButtonElement>('.visual-prev')!.click();
    await settled(host, () => loaded.url() !== offline && prepared.get(loaded.url() ?? '')?.live === live, () => `the previous version did not load with Live ${live} after .visual-prev (offline page ${offline}, which ${before} reported loading at the click): ${seen()}`);
    equal(host.querySelector('iframe'), frame);
    equal(frame.getAttribute('sandbox'), 'allow-scripts');
    equal(host.querySelector('.side-meta')!.textContent, t(live ? 'visual.live' : 'visual.offline'));
    await api.saveSettings({ webSearch: 'off' });
    host.querySelector<HTMLButtonElement>('.visual-next')!.click();
    await settled(host, () => prepared.get(loaded.url() ?? '')?.live === false, () => `the next version did not load offline after .visual-next: ${seen()}`);
    equal(host.querySelector('iframe'), frame);
    equal(calls, 3);
  } finally { dispose(); host.remove(); loaded.stop(); }
  equal(host.querySelector('iframe'), null);
});

check('visual answers: Linux opens a real offline page even when Web search is Live', async () => {
  if (!/Linux/.test(navigator.userAgent)) return;
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  let result: { ready: boolean; blocked: boolean } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  await api.saveSettings({ webSearch: 'live' });
  const a = answer();
  a.versions = [{ ...a.versions[0]!, source: `<p id="status">Waiting</p><script>
    document.addEventListener('DOMContentLoaded',async()=>{
      document.getElementById('status').textContent='Ready';
      let blocked=false;try{await fetch('https://visual-fixture.example/linux');}catch{blocked=true;}
      parent.postMessage({probe:${JSON.stringify(probe)},ready:document.getElementById('status').textContent==='Ready',blocked},'*');
    });</script>` }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'linux', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result, 'Linux must render and execute the offline visual, including with saved Live');
    equal(result!.ready, true); equal(result!.blocked, true);
    equal(await api.visualPolicy(), false, 'the GTK navigation policy never enables live network access');
    equal(prepared.get(host.querySelector('iframe')!.src)!.live, false);
  } finally { dispose(); host.remove(); window.removeEventListener('message', listen); }
});
const source = (n: number) => `<!doctype html><html lang="en"><head><title>Budget chart</title></head><body data-version="${n}" style="margin:0"><p>Version ${n}</p><button onclick="this.textContent='Changed'">Change</button></body></html>`;
const fence = (text: string) => `\`\`\`html\n${text}\n\`\`\``;
const answer = (): VisualAnswer => ({ id: 'v1:0', name: 'Budget chart', kind: 'html', versions: [1, 2].map((n) => ({ messageId: `v${n}`, block: 0, start: 0, end: fence(source(n)).length, kind: 'html', source: source(n) })) });
const messages = (): Message[] => [1, 2].map((n) => ({ id: `v${n}`, role: 'assistant', text: fence(source(n)), vendor: n === 1 ? 'claude' : 'codex', at: n, status: 'done' }));

check('visual files: cards retain exact versions and show the self-contained limit without duplicating reply cards', async () => {
  const a = answer();
  a.name = 'site/index.html';
  a.versions = a.versions.map((v) => ({ ...v, file: 'site/index.html', start: 0, end: 0 }));
  const host = document.createElement('div'); document.body.append(host);
  const dispose = render(() => createComponent(VisualFiles, { answers: [a], messageId: 'v1', conversationId: 'files' }), host);
  const reply = document.createElement('div'); host.append(reply);
  const stopReply = render(() => createComponent(VisualReply, { text: 'Files ready.', offset: 0, messageId: 'v1', conversationId: 'files', answers: [a] }), reply);
  try {
    equal(host.querySelectorAll('.visual-card').length, 1);
    ok(host.textContent!.includes(t('visual.fileOnly')));
    host.querySelector<HTMLButtonElement>('.visual-open')!.click();
    equal(openedVisual()!.messageId, 'v1'); equal(openedVisual()!.file, 'site/index.html');
    host.querySelector('details')!.open = true;
    await settle();
    ok(host.querySelector('pre')!.textContent!.includes('Version 1'));
    ok(!host.querySelector('pre')!.textContent!.includes('Version 2'));
  } finally { stopReply(); dispose(); host.remove(); showVisual(undefined); }
});

check('visual files: discovery has no source and opening code or a version reads only that saved version', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const loaded = loadedVisual(host);
  const a = answer();
  a.versions = a.versions.map((v) => ({ ...v, file: 'page.html', start: 0, end: 0, source: '' }));
  const saved = api.visualAnswers;
  const requests: [string, string | undefined, number | undefined][] = [];
  api.visualAnswers = async (id, messageId, block) => {
    requests.push([id, messageId, block]);
    return [{ ...a, versions: a.versions.map((v) => ({ ...v, source: v.messageId === messageId && v.block === block ? loaded.source(source(messageId === 'v1' ? 1 : 2)) : '' })) }];
  };
  let dispose = render(() => createComponent(VisualFiles, { answers: [a], messageId: 'v1', conversationId: 'lazy-files' }), host);
  try {
    equal(requests.length, 0);
    host.querySelector('details')!.open = true;
    await until(() => host.querySelector('pre'));
    deepEqual(requests, [['lazy-files', 'v1', 0]]);
    ok(host.querySelector('pre')!.textContent!.includes('Version 1'));
    host.querySelector('details')!.open = false;
    await until(() => !host.querySelector('pre'));
    dispose();
    dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'lazy-files', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
    await until(() => host.querySelector('iframe'));
    const frame = host.querySelector('iframe')!;
    ok(prepared.get(frame.src)!.document.includes('Version 2'));
    host.querySelector<HTMLButtonElement>('.visual-prev')!.click();
    await until(() => prepared.get(loaded.url() ?? '')?.document.includes('Version 1'));
    deepEqual(requests, [['lazy-files', 'v1', 0], ['lazy-files', 'v2', 0], ['lazy-files', 'v1', 0]]);
    ok(a.versions.every((v) => !v.source), 'discovery never becomes a second source store');
  } finally { dispose(); host.remove(); api.visualAnswers = saved; loaded.stop(); }
});

check('visual answers: a missing navigation window shows the existing failure and another version recovers', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const loaded = loadedVisual(host);
  const a = answer();
  a.versions = a.versions.map((v) => ({ ...v, source: loaded.source(v.source) }));
  const dispose = render(() => createComponent(ErrorBoundary, { fallback: () => { const error = document.createElement('p'); error.dataset.navigationCrash = ''; return error; }, get children() { return createComponent(VisualPane, { answer: a, open: { conversationId: 'navigation-failure', answerId: a.id }, onBack: () => {}, onClose: () => {} }); } }), host);
  try {
    await until(() => loaded.url());
    const first = loaded.url();
    Object.defineProperty(host.querySelector('iframe')!, 'contentWindow', { configurable: true, get: () => null });
    host.querySelector<HTMLButtonElement>('.visual-prev')!.click();
    await until(() => host.querySelector('[role="alert"], [data-navigation-crash]'));
    equal(host.querySelector('[data-navigation-crash]'), null, 'navigation failures stay inside the pane');
    equal(host.querySelector('[role="alert"]')!.textContent, t('visual.failed'));
    equal(host.querySelector('iframe'), null);
    host.querySelector<HTMLButtonElement>('.visual-next')!.click();
    await until(() => loaded.url() !== first && host.querySelector('iframe'));
    equal(host.querySelector('[role="alert"]'), null);
  } finally { dispose(); host.remove(); loaded.stop(); }
});

check('Mermaid: source cannot escape into markup, the lazy library is pinned and its code fallback stays offline', () => {
  const source = 'flowchart LR\nA-->B\n</pre><script>parent.compromised=true</script>';
  const doc = new DOMParser().parseFromString(mermaidDocument(source), 'text/html');
  equal(doc.querySelector('#source')!.textContent, source);
  equal(doc.querySelectorAll('script').length, 2);
  const library = doc.querySelector('script[src]')!;
  equal(library.getAttribute('src'), MERMAID_URL);
  equal(library.getAttribute('integrity'), MERMAID_INTEGRITY);
  equal(library.getAttribute('crossorigin'), 'anonymous');
  ok(!doc.querySelector('#source')!.hasAttribute('hidden'));
  const offline = new DOMParser().parseFromString(visualDocument(mermaidDocument('flowchart LR\nA-->B')), 'text/html');
  equal(offline.querySelector('[data-faber-src]'), null, 'offline cleaning removes the only library request');
  equal(offline.querySelector('#source')!.textContent, 'flowchart LR\nA-->B');
  const live = new DOMParser().parseFromString(visualDocument(mermaidDocument('flowchart LR\nA-->B'), true), 'text/html');
  equal(live.querySelector('[data-faber-src]')!.getAttribute('integrity'), MERMAID_INTEGRITY);
  equal(live.querySelectorAll('[data-faber-src]').length, 1);
});

check('Mermaid: reply cards do not keep a permanent renderer-limits notice', () => {
  const source = 'flowchart LR\nA-->B';
  const text = '```mermaid\n' + source + '\n```';
  const a = answer(); a.kind = 'mermaid';
  a.versions = [{ ...a.versions[0]!, kind: 'mermaid', source, start: 0, end: text.length }];
  const host = document.createElement('div'); document.body.append(host);
  const dispose = render(() => createComponent(VisualReply, { text, offset: 0, messageId: 'v1', conversationId: 'mermaid', answers: [a] }), host);
  try {
    equal(host.querySelectorAll('.visual-card').length, 1);
    equal(host.querySelector('.visual-card .side-meta'), null);
    ok(!host.textContent!.includes(t('visual.mermaidWeb')));
  } finally { dispose(); host.remove(); }
});

check('Mermaid: a controlled renderer draws in the real isolated pane and a render failure retains code', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const saved = api.visualPage;
  const probe = crypto.randomUUID();
  let result: { drawing: boolean; code: boolean; notice: boolean; isolated: boolean } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  try {
    for (const fails of [false, true]) {
      result = undefined;
      // Deterministic screen fixture: no external service or third-party bytes enter the test build.
      api.visualPage = (id, revision, document) => {
        const doc = new DOMParser().parseFromString(document, 'text/html');
        const library = doc.querySelector('script[data-faber-src]')!;
        library.removeAttribute('data-faber-src');
        library.textContent = `window.mermaid={initialize(c){if(c.securityLevel!=='strict'||c.maxEdges!==200)throw Error('unsafe config');},render(){return ${fails ? "Promise.reject(Error('fixture'))" : "Promise.resolve({svg:'<svg xmlns=\"http://www.w3.org/2000/svg\"><title>Flow</title><text>A to B</text></svg>'})"};}};`;
        const report = doc.createElement('script');
        report.type = 'application/faber-script'; report.setAttribute('data-faber-script', 'text/javascript');
        report.textContent = `window.addEventListener('load',()=>{let isolated=false;try{parent.document.body}catch{isolated=true}parent.postMessage({probe:${JSON.stringify(probe)},drawing:!!document.querySelector('#drawing svg'),code:!document.getElementById('source').hidden,notice:!document.getElementById('fallback').hidden,isolated},'*')});`;
        doc.body.append(report);
        return saved(id, revision, '<!doctype html>' + doc.documentElement.outerHTML);
      };
      const a = answer(); a.kind = 'mermaid'; a.versions = [{ ...a.versions[0]!, kind: 'mermaid', source: 'flowchart LR\nA-->B' }];
      const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'diagram', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
      try {
        await until(() => result);
        equal(result!.drawing, !fails); equal(result!.code, fails); equal(result!.notice, fails); equal(result!.isolated, true);
        equal(host.querySelector('iframe')!.getAttribute('sandbox'), 'allow-scripts');
        equal(prepared.get(host.querySelector('iframe')!.src)!.live, false);
      } finally { dispose(); }
    }
  } finally { api.visualPage = saved; host.remove(); window.removeEventListener('message', listen); }
});

registerCheck('visual answers: the real native filter compiles before Live can be used on Windows and macOS', async () => {
  if (!/Windows|Macintosh|Mac OS/.test(navigator.userAgent)) return;
  const web = (await api.state()).settings.webSearch ?? 'live';
  let id: string | undefined;
  try {
    await api.saveSettings({ webSearch: 'live' });
    const until = performance.now() + 5000;
    while (!(await api.visualPolicy()) && performance.now() < until) await settle();
    ok(await api.visualPolicy(), 'the actual WebKit rule compilation or WebView2 guard installation must succeed');
    id = await api.visualOpen();
    const page = await api.visualPage(id, 1, visualDocument('<p>Native guard</p>', true));
    equal(page.live, true, 'a failed native install cannot silently turn this test into an offline check');
    ok(page.url.includes('faber-visual'));
  } finally {
    if (id) await api.visualClose(id);
    await api.saveSettings({ webSearch: web });
  }
});

check('visual answers: named forms cannot replace the serialized root or blocking policy and no host nonce enters the page', () => {
  const marker = document.createElement('meta');
  marker.name = 'faber-visual-nonce'; marker.content = 'host-secret-probe';
  document.head.append(marker);
  try {
    for (const name of ['documentElement', 'head', 'body', 'close', 'querySelectorAll']) {
      const made = visualDocument(`<form name="${name}"><input name="attributes"><input name="setAttribute"><script nonce="authored" src="https://example.com/probe.js"></script></form>`);
      ok(made.startsWith('<!doctype html><html'), `the real document is serialized for ${name}`);
      const template = document.createElement('template'); template.innerHTML = made;
      equal(template.content.querySelector('meta[http-equiv="Content-Security-Policy"]')!.getAttribute('content'), VISUAL_CSP);
      equal(template.content.querySelector('[nonce]'), null, 'even authored nonces are removed');
      equal(template.content.querySelector('script[src]'), null, 'external scripts are never granted authorization');
      ok(!made.includes(marker.content), 'the host authorization never enters srcdoc');
    }
  } finally { marker.remove(); }
});

check('visual answers: static connection hints and nested template hints are removed before the page is parsed', () => {
  const hints = ['dns-prefetch', 'preconnect', 'prefetch', 'prerender', 'modulepreload'];
  const links = hints.map((rel) => `<link rel="${rel}" href="https://example.com/probe">`).join('');
  const made = visualDocument(`${links}<template>${links}<template>${links}</template></template><script type="speculationrules">{"prefetch":[{"source":"list","urls":["https://example.com"]}]}</script>`);
  ok(!/<link\b/i.test(made), 'templates cannot preserve a hint for later cloning');
  ok(!made.includes('speculationrules'), 'speculation rules cannot bypass the offline policy');
});

check('visual answers: scripting-dependent noscript markup is dropped, including named forms and nested templates', () => {
  const payload = `<noscript><p title="</noscript><iframe id='noscript-frame'></iframe>">ignored</p></noscript>`;
  for (const live of [false, true]) {
    const made = visualDocument(`<body>${payload}<form name="parentNode"><input name="parentNode">${payload}</form><template>${payload}<template>${payload}</template></template>`, live);
    ok(!/<noscript\b/i.test(made), 'nothing whose parsing changes with scripting survives serialization');
    ok(!made.includes('noscript-frame'), 'the hidden second-parse frame is dropped with its noscript parent');
    equal(new DOMParser().parseFromString(made, 'text/html').querySelector('iframe,frame,object,embed'), null);
  }
});

check('visual answers: declarative open and closed shadows cannot hide a nested frame from the real parser', async () => {
  for (const mode of ['open', 'closed']) {
    const host = document.createElement('div'); document.body.append(host);
    const probe = crypto.randomUUID();
    let result: { frames: number; shadows: number; templates: number } | undefined;
    const listen = (e: MessageEvent) => {
      if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
    };
    window.addEventListener('message', listen);
    const saved = api.visualPage;
    // Keep the cleaner's template attributes, but restore nested contexts to exercise the real second parser too.
    api.visualPage = (id, revision, document) => saved(id, revision, document.replaceAll('<p class="shadow-frame-probe"></p>',
      '<script type="application/json"></script><iframe></iframe>'));
    const a = answer();
    a.versions = [{ ...a.versions[0]!, source: `<body>
      <div><template shadowrootmode="${mode}"><p class="shadow-frame-probe"></p><template shadowroot="${mode}" shadowrootmode="${mode}"><p class="shadow-frame-probe"></p></template></template></div>
      <div><template shadowroot="${mode}"><p class="shadow-frame-probe"></p></template></div>
      <script>document.addEventListener('DOMContentLoaded',()=>{
        let shadows=0,frames=window.length;
        const walk=(root)=>{for(const el of root.querySelectorAll('*')){if(el.shadowRoot){shadows++;frames+=el.shadowRoot.querySelectorAll('iframe,frame').length;walk(el.shadowRoot);}}};
        walk(document);
        parent.postMessage({probe:${JSON.stringify(probe)},frames,shadows,templates:document.querySelectorAll('template').length},'*');
      });</script>` }];
    const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: `shadow-${mode}`, answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
    try {
      await until(() => result, `the ${mode} declarative shadow probe reports from the actual frame`);
      equal(result!.frames, 0, 'no nested browsing realm survives, including inside a closed tree');
      equal(result!.shadows, 0, 'declarative templates stay inert instead of becoming hidden parser destinations');
      equal(result!.templates, 2, 'the original templates remain available as inert page data');
      ok(!/\sshadowroot(?:mode)?=/i.test(visualDocument(a.versions[0]!.source, true)), 'including nested and legacy template attributes');
    } finally { api.visualPage = saved; dispose(); host.remove(); window.removeEventListener('message', listen); }
  }
});

check('visual answers: the early native guard removes parser-created nested contexts and template clones in the real frame', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  let result: { parser: number; cloned: number; transports: boolean; error?: string } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  const saved = api.visualPage;
  // Bypass the detached cleaner only in this check: parser-created nodes must meet the native guard too.
  api.visualPage = (id, revision, document) => saved(id, revision, document.replace('<p id="parser-guard-probe"></p>',
    '<iframe></iframe><object></object><embed><svg><foreignObject><iframe></iframe></foreignObject></svg><template id="parser-template"><iframe></iframe><template><object></object><embed></template></template>'));
  const a = answer();
  a.versions = [{ ...a.versions[0]!, source: `<body><p id="parser-guard-probe"></p>
    <noscript><p title="</noscript><iframe id='noscript-frame'></iframe>">ignored</p></noscript>
    <template><noscript><p title="</noscript><iframe id='template-noscript-frame'></iframe>">ignored</p></noscript></template>
    <script>document.addEventListener('DOMContentLoaded',()=>{
      const report={probe:${JSON.stringify(probe)},parser:0,cloned:0,transports:false};
      try {
        const denied='iframe,frame,object,embed,noscript';
        report.parser=document.querySelectorAll(denied).length;
        for(const template of document.querySelectorAll('template')){
          document.body.append(template.content.cloneNode(true));
        }
        report.cloned=document.querySelectorAll(denied).length;
        report.transports=['WebSocket','WebSocketStream','WebTransport','RTCPeerConnection'].every(name=>window[name]===undefined);
      }catch(error){report.error=String(error);}
      parent.postMessage(report,'*');
    });</script>` }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'parser-guard', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result, 'the parser-guard page reports from the actual opaque frame');
    equal(result!.error, undefined);
    equal(result!.parser, 0, 'HTML parser insertions cannot leave a nested browser realm');
    equal(result!.cloned, 0, 'template contents cannot restore a nested browser realm');
    equal(result!.transports, true, 'the remaining authored realm keeps all transport guards');
  } finally { api.visualPage = saved; dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: a parser-created closed shadow cannot supply an unguarded nested realm, including SVG and MathML contexts', async () => {
  if (!('shadowRootMode' in HTMLTemplateElement.prototype) || typeof ElementInternals === 'undefined') return;
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  const name = `hidden-${probe}`;
  let result: { rootBuilt: boolean; frameBuilt: boolean; access: boolean; named: boolean; cleaned: boolean; locked: boolean; visible: number; error?: string } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  const saved = api.visualPage;
  api.visualPage = (id, revision, document) => {
    ok(document.includes('<p id="mutation-probe"></p>'), 'the closed-shadow parser fixture has its insertion point');
    // Inject fixed parser bytes after cleaning: modern outerHTML escapes attribute angles and defeats the old mXSS fixture.
    const markup = `<x-visual-attack><template shadowrootmode="closed"><x-frame-witness></x-frame-witness><iframe name="${name}"></iframe></template></x-visual-attack>`;
    // Register a witness before the body parser: capture the real iframe even if another defense detaches it.
    const witness = `const proof=window.closedFrameProof={frames:[],constructed:0,connected:0};
      customElements.define('x-frame-witness',class extends HTMLElement{
        constructor(){super();proof.constructed++;}
        connectedCallback(){
          proof.connected++;
          const root=this.getRootNode();proof.root=root;
          const capture=(records)=>{for(const record of records)for(const node of record.addedNodes){if(node.localName==='iframe')proof.frames.push(node);}};
          const observer=new MutationObserver(capture);observer.observe(root,{childList:true});
          proof.drain=()=>{capture(observer.takeRecords());observer.disconnect();};
        }
      });`;
    const made = document.replace('<p id="mutation-probe"></p>', markup
      + '<svg><foreignObject><iframe srcdoc="nested"></iframe></foreignObject></svg><math><mtext><iframe srcdoc="nested"></iframe></mtext></math>');
    return saved(id, revision, made.replace('<script>', `<script type="application/faber-events" data-faber-events>${witness}</script><script>`));
  };
  const a = answer();
  // Define the witness before the parser reaches the body; authored startup still uses the real loader.
  a.versions = [{ ...a.versions[0]!, source: `<head><script>
    document.addEventListener('DOMContentLoaded',()=>{
      const report={probe:${JSON.stringify(probe)},rootBuilt:false,frameBuilt:false,access:false,named:false,cleaned:false,locked:false,visible:document.querySelectorAll('iframe,frame,object,embed').length};
      try {
        const proof=window.closedFrameProof;proof.drain?.();
        report.rootBuilt=proof.root instanceof ShadowRoot && proof.root.mode==='closed';
        if(!report.rootBuilt)throw Error('parser witness missed the closed root: constructed='+proof.constructed+', connected='+proof.connected);
        report.frameBuilt=proof.frames.length>0;
        report.access=report.frameBuilt && proof.frames.every(frame=>frame.contentWindow===null && frame.contentDocument===null);
        report.named=window.open('',${JSON.stringify(name)})===null;
        report.locked=Object.getOwnPropertyDescriptor(window,'open').configurable===false;
        report.locked=report.locked && Object.getOwnPropertyDescriptor(ElementInternals.prototype,'shadowRoot').configurable===false;
        for(const Constructor of [HTMLIFrameElement,HTMLFrameElement]){
          for(const key of ['contentWindow','contentDocument'])report.locked=report.locked && Object.getOwnPropertyDescriptor(Constructor.prototype,key).configurable===false;
        }
        customElements.define('x-visual-attack',class extends HTMLElement{
          constructor(){super();proof.internals=this.attachInternals().shadowRoot;}
        });
        // Upgrade the exact host retained by the parser witness, rather than querying the document again.
        if(proof.root.host.localName!=='x-visual-attack' || proof.root.host.namespaceURI!=='http://www.w3.org/1999/xhtml')throw Error('the witness must retain the actual HTML attack host');
        customElements.upgrade(proof.root.host);
        report.cleaned=proof.internals===proof.root && !proof.root.querySelector('iframe');
      }catch(error){report.error=String(error);}
      parent.postMessage(report,'*');
    });</script></head><body><p id="mutation-probe"></p>` }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'closed-shadow-parser', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result, 'the closed-shadow parser probe reports from the actual visual frame');
    equal(result!.error, undefined);
    equal(result!.rootBuilt, true, 'the actual frame parser constructed the closed shadow root');
    equal(result!.frameBuilt, true, 'the witness observed the actual iframe inside that closed tree');
    equal(result!.access, true, 'frame accessors cannot expose its initial empty realm, even without child initialization');
    equal(result!.named, true, 'opening an existing named child cannot expose its realm');
    equal(result!.cleaned, true, 'ElementInternals cleans the closed root before returning it');
    equal(result!.locked, true, 'the parent-realm access guards cannot be replaced');
    equal(result!.visible, 0, 'SVG foreignObject and MathML parser-created frames are removed');
  } finally { api.visualPage = saved; dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: parser-created document frames expose no indexed or named child to the first authored script', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID(), name = `parser-${probe}`;
  let result: { built: boolean; indexed: boolean; frames: boolean; named: boolean; children: number; inline: boolean } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  const saved = api.visualPage;
  api.visualPage = (id, revision, document) => {
    // Bypass the detached cleaner to exercise the real parser and the native guard's last drain.
    const markup = `<iframe name="${name}"></iframe><script>window.parserSiblingRan=true;</script>`;
    const witness = `const proof=window.parserFrameProof={built:false};
      const capture=(records)=>{for(const record of records)for(const node of record.addedNodes){
        if(node instanceof HTMLIFrameElement && node.getAttribute('name')===${JSON.stringify(name)})proof.built=true;
      }};
      const observer=new MutationObserver(capture);observer.observe(document,{childList:true,subtree:true});
      proof.drain=()=>{capture(observer.takeRecords());observer.disconnect();};`;
    const made = document.replace('<p id="parser-probe"></p>', markup);
    return saved(id, revision, made.replace('<script>', `<script type="application/faber-events" data-faber-events>${witness}</script><script>`));
  };
  const a = answer();
  a.versions = [{ ...a.versions[0]!, source: `<head><script>
    window.parserFrameProof.drain();
    parent.postMessage({probe:${JSON.stringify(probe)},built:window.parserFrameProof.built,indexed:window[0]===undefined,
      frames:window.frames[0]===undefined,named:window[${JSON.stringify(name)}]===undefined,
      children:document.querySelectorAll('iframe,frame,object,embed').length,
      inline:window.parserSiblingRan===true},'*');
    </script></head><body><p id="parser-probe"></p>` }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'parser-access', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result, 'the very first authored script reports after the native parser drain');
    equal(result!.built, true, 'the parser witness observed the actual named iframe node');
    equal(result!.inline, false, 'the hash policy blocks an untrusted sibling inline script during parsing');
    equal(result!.indexed, true, 'window[0] has no child realm');
    equal(result!.frames, true, 'window.frames[0] has no child realm');
    equal(result!.named, true, 'a named frame has no child realm');
    equal(result!.children, 0, 'parser-created document frames are gone before authored replay');
  } finally { api.visualPage = saved; dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: meta refresh, window.open, workers and CSS requests cannot leave the offline frame', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  let result: { popup: boolean; worker: boolean; shared: boolean; cssImport: boolean; cssUrl: boolean; refresh: boolean } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  const a = answer();
  a.versions = [{ ...a.versions[0]!, source: `<meta http-equiv="refresh" content="0;url=data:text/html,unexpected"><p>Sandbox probes</p><script>
    document.addEventListener('DOMContentLoaded',async()=>{
      const violations=[];document.addEventListener('securitypolicyviolation',e=>violations.push({uri:e.blockedURI,directive:e.effectiveDirective}));
      const style=document.createElement('style');style.textContent='@import url("https://visual-fixture.example/import.css");body{background-image:url("https://visual-fixture.example/background.svg")}';document.head.append(style);
      const url=URL.createObjectURL(new Blob(['postMessage("unexpected");'],{type:'text/javascript'}));
      const denied=async(name)=>{let worker;try{return await new Promise(resolve=>{worker=new window[name](url);const target=worker.port||worker;target.onmessage=()=>resolve(false);worker.onerror=e=>{e.preventDefault();resolve(true);};target.start?.();setTimeout(()=>resolve(false),500);});}catch{return true;}finally{worker?.terminate?.();worker?.port?.close();}};
      const popup=window.open('about:blank')===null;
      const worker=await denied('Worker'),shared=await denied('SharedWorker');URL.revokeObjectURL(url);
      await new Promise(resolve=>setTimeout(resolve,50));
      parent.postMessage({probe:${JSON.stringify(probe)},popup,worker,shared,
        cssImport:violations.some(e=>e.uri.endsWith('/import.css')&&e.directive.startsWith('style-src')),
        cssUrl:violations.some(e=>e.uri.endsWith('/background.svg')&&e.directive==='img-src'),
        refresh:!document.querySelector('meta[http-equiv="refresh"]')&&location.protocol!=='data:'},'*');
    });</script>` }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'browser-surfaces', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result, 'the actual frame reports all remaining browser-surface probes');
    for (const name of ['popup', 'worker', 'shared', 'cssImport', 'cssUrl', 'refresh'] as const) equal(result![name], true, `${name} stays confined`);
    const policy = prepared.get(host.querySelector('iframe')!.src)!.document;
    for (const directive of ["frame-src 'none'", "child-src 'none'", "worker-src 'none'"]) ok(policy.includes(directive));
  } finally { dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: scripts cannot connect dynamic hints through DOM, HTML, templates, ranges or document writes', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  let result: { cases: string[]; error?: string } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  const script = `<script>
    const cases=[];
    const safe=(name)=>{
      if(document.querySelector('link,iframe,frame,object,embed,script[type="speculationrules"]')) throw Error(name);
      cases.push(name);
    };
    try {
      for(const rel of ['dns-prefetch','preconnect','prefetch','prerender','modulepreload']) {
        const link=document.createElement('link'); link.rel=rel; link.href='https://example.com/dynamic-probe';
        document.head.appendChild(link); safe('createElement '+rel);
      }
      const markup='<link rel="dns-prefetch" href="//example.com"><link rel="preconnect" href="https://example.com">';
      const box=document.createElement('div'); document.body.append(box);
      box.innerHTML=markup; safe('innerHTML');
      box.insertAdjacentHTML('beforeend',markup); safe('insertAdjacentHTML');
      box.outerHTML='<div>'+markup+'</div>'; safe('outerHTML');
      const svg=document.createElementNS('http://www.w3.org/2000/svg','svg'); document.body.append(svg);
      const foreign='<style><img>'+markup+'</style>';
      svg.innerHTML=foreign; safe('SVG innerHTML final parser context');
      svg.insertAdjacentHTML('beforeend',foreign); safe('SVG adjacent HTML final parser context');
      const template=document.createElement('template'); template.innerHTML='<template>'+markup+'</template>'+markup;
      document.body.append(template.content.cloneNode(true));
      document.body.append(document.querySelector('template').content.cloneNode(true)); safe('nested template clone');
      const range=document.createRange(); range.selectNodeContents(document.body);
      range.insertNode(range.createContextualFragment(markup)); safe('range fragment and insertNode');
      const parsed=new DOMParser().parseFromString('<body>'+markup+'</body>','text/html');
      document.body.append(document.adoptNode(parsed.body)); safe('DOMParser and adoptNode');
      document.head.prepend(document.createElementNS('http://www.w3.org/1999/xhtml','x:link')); safe('qualified name');
      const linked=document.implementation.createDocument('http://www.w3.org/1999/xhtml','link').documentElement;
      linked.setAttribute('rel','preconnect'); linked.setAttribute('href','https://example.com');
      document.head.appendChild(linked); safe('XML root cannot bypass node insertion');
      const shadowHost=document.createElement('div'); document.body.append(shadowHost);
      const shadow=shadowHost.attachShadow({mode:'open'}); shadow.innerHTML=markup;
      if(shadow.querySelector('link')) throw Error('shadow'); safe('shadow');
      const closedHost=document.createElement('div');document.body.append(closedHost);
      const closed=closedHost.attachShadow({mode:'closed'});closed.innerHTML=markup+'<iframe></iframe>';
      if(closed.querySelector('link,iframe'))throw Error('closed shadow');
      const attach=Object.getOwnPropertyDescriptor(Element.prototype,'attachShadow');
      if(attach.configurable || attach.writable)throw Error('the shadow guard can be replaced');safe('closed shadow');
      const script=document.createElement('script'); script.type='application/json';
      script.textContent=JSON.stringify({prefetch:[{source:'list',urls:['https://example.com']}]}); document.body.append(script);
      script.type='speculationrules'; safe('script type setter');
      script.setAttribute('type','speculationrules'); safe('script attribute');
      const attr=document.createAttribute('type'); attr.value='speculationrules'; script.setAttributeNode(attr); safe('attribute node');
      script.attributes.getNamedItem('type').value='speculationrules'; safe('connected attribute value');
      script.attributes.getNamedItem('type').textContent='speculationrules'; safe('connected attribute text');
      document.write(markup); safe('document.write');
      document.open(); document.writeln(markup); safe('document.open and writeln');
      parent.postMessage({probe:${JSON.stringify(probe)},cases},'*');
    } catch(error) { parent.postMessage({probe:${JSON.stringify(probe)},cases,error:String(error)},'*'); }
  </script>`;
  const a = answer(); a.versions = [{ ...a.versions[0]!, source: script, file: 'project/app.html' }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'hints', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result);
    equal(result!.error, undefined, result!.error);
    equal(result!.cases.length, 24, 'every insertion path is checked synchronously, before an observer could remove a connected hint');
  } finally { dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: a connected stylesheet cannot become a connection hint through reflected properties, attributes or tokens', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  let result: { count: number; error?: string } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  const source = `<script>
    let count=0,error;
    try {
      const link=document.createElement('link'); link.rel='stylesheet'; document.head.append(link);
      if(link.localName!=='link' || !link.isConnected) throw Error('normal stylesheets must remain usable');
      const changes=[
        ()=>link.rel='preconnect',
        ()=>link.setAttribute('rel','dns-prefetch'),
        ()=>link.setAttributeNS(null,'rel','prerender'),
        ()=>{const attr=document.createAttribute('rel'); attr.value='prefetch'; link.setAttributeNode(attr);},
        ()=>{const attr=document.createAttribute('rel'); attr.value='modulepreload'; link.attributes.setNamedItem(attr);},
        ()=>link.getAttributeNode('rel').value='preconnect',
        ()=>link.getAttributeNode('rel').textContent='prefetch',
        ()=>link.getAttributeNode('rel').nodeValue='dns-prefetch',
        ()=>link.relList.value='preconnect',
        ()=>link.relList='prefetch',
        ()=>link.relList.add('stylesheet','preconnect'),
        ()=>link.relList.replace('stylesheet','prerender'),
        ()=>link.relList.toggle('modulepreload',true),
        ()=>{let n=0;link.relList.add({toString(){return ++n===1?'stylesheet':'dns-prefetch';}});},
        ()=>{let n=0;link.relList.replace('stylesheet',{toString(){return ++n===1?'stylesheet':'preconnect';}});},
        ()=>{let n=0;link.relList.toggle({toString(){return ++n===1?'stylesheet':'prefetch';}},true);},
        ()=>{let n=0;link.setAttributeNS(null,{toString(){return ++n===1?'title':'rel';}},'dns-prefetch');},
        ()=>{WeakSet.prototype.has=()=>false;WeakSet.prototype.add=()=>{};link.relList.add('preconnect');},
      ];
      for(const change of changes) {
        link.rel='stylesheet'; change();
        if(!['','stylesheet'].includes(link.getAttribute('rel'))) throw Error('hint escaped at '+count);
        count++;
      }
    } catch(e) {error=String(e);}
    parent.postMessage({probe:${JSON.stringify(probe)},count,error},'*');
  </script>`;
  const a = answer(); a.versions = [{ ...a.versions[0]!, source }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'stylesheet', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result);
    equal(result!.error, undefined);
    equal(result!.count, 18);
    const live = new DOMParser().parseFromString(visualDocument('<link rel="stylesheet" href="https://cdn.example.com/style.css"><link rel="stylesheet preconnect" href="https://example.com">', true), 'text/html');
    equal(live.querySelectorAll('link').length, 1);
    equal(live.querySelector('link')!.rel, 'stylesheet');
  } finally { dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: focused input Escape and right-click cross only the validated dismissal boundary at 720 px', async () => {
  const host = document.createElement('div'); host.style.width = '720px'; document.body.append(host);
  const probe = crypto.randomUUID();
  let ready = false;
  let result: { prevented: boolean } | undefined;
  let closed = 0;
  const listen = (e: MessageEvent) => {
    if (e.source !== host.querySelector('iframe')?.contentWindow || e.data?.probe !== probe) return;
    if (e.data.ready) ready = true; else result = e.data;
  };
  window.addEventListener('message', listen);
  const script = `<input id="field"><script>
    addEventListener('message',e=>{
      if(e.data!==${JSON.stringify(probe)}) return;
      const input=document.querySelector('#field'); input.focus();
      const event=new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2}); input.dispatchEvent(event);
      parent.postMessage({probe:${JSON.stringify(probe)},prevented:event.defaultPrevented},'*');
      input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
    });
    parent.postMessage({probe:${JSON.stringify(probe)},ready:true},'*');
  </script>`;
  const a = answer(); a.versions = [{ ...a.versions[0]!, source: script }];
  const [visible, setVisible] = createSignal(true);
  const dispose = render(() => createComponent(Show, { keyed: true, get when() { return visible(); }, get children() {
    return createComponent(VisualPane, { answer: a, open: { conversationId: 'keys', answerId: a.id }, onBack: () => {}, onClose: () => { closed++; setVisible(false); } });
  } }), host);
  try {
    await until(() => ready);
    await until(() => host.querySelector('iframe'));
    const frame = host.querySelector('iframe')!;
    const send = (data: unknown, origin = 'null', source: MessageEventSource | null = frame.contentWindow) => window.dispatchEvent(new MessageEvent('message', { data, origin, source }));
    send({ type: 'faber-visual-escape' }, 'https://example.com');
    send({ type: 'faber-visual-escape' }, 'null', window);
    send({ type: 'faber-visual-escape', command: 'state' });
    send({ type: 'state' }); send(['faber-visual-escape']);
    equal(closed, 0, 'wrong origins, sources, commands and extra fields cannot act on Faber');
    openMenu(new MouseEvent('contextmenu', { button: 2 }), [{ id: 'probe', label: 'Probe', run: () => {} }]);
    ok(menu());
    send({ type: 'faber-visual-contextmenu', command: 'state' });
    ok(menu(), 'an extra field also invalidates context-menu dismissal');
    frame.contentWindow!.postMessage(probe, '*');
    await until(() => result && closed === 1);
    // WebKit leaves focus where it was in a hidden window (macOS CI), so the keys are dispatched on the input itself.
    equal(result!.prevented, true, 'the native context menu is canceled inside the frame');
    equal(menu(), undefined, 'the narrow context-menu message closes an existing Faber menu');
    equal(host.querySelector('iframe'), null, 'Escape releases the frame');
    send({ type: 'faber-visual-escape' });
    equal(closed, 1, 'a removed frame cannot dismiss anything else');
  } finally { closeMenu(); dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: closed fences become cards, the existing code view stays available, and ordinary Markdown keeps its references and DOM', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const [text, setText] = createSignal('See [docs][d]\n\n' + fence(source(1)).slice(0, -3));
  const [visuals, setVisuals] = createSignal<VisualAnswer[]>([]);
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [{ ...messages()[0]!, text: text(), status: 'streaming' as const }]; },
    get visuals() { return visuals(); }, conversationId: 'visual', options: [], from: 0, onBranch: () => {},
  }), host);
  try {
    equal(host.querySelector('.visual-card'), null, 'an open fence remains code');
    ok(host.querySelector('.codeblock'));
    const paragraph = host.querySelector('.md p');
    setText(text() + '\n');
    equal(host.querySelector('.md p'), paragraph, 'unrelated streaming keeps existing Markdown nodes');
    const prefix = 'See [docs][d]\n\n';
    setText(prefix + fence(source(1)) + '\n\n[d]: https://example.com');
    const a = answer();
    a.versions = [{ ...a.versions[0]!, start: prefix.length, end: prefix.length + fence(source(1)).length }];
    setVisuals([a]);
    equal(host.querySelectorAll('.visual-card').length, 1);
    equal(host.querySelector('.visual-card b')!.textContent, 'Budget chart');
    equal(host.querySelector('pre'), null, 'the card does not keep hidden code DOM');
    equal(host.querySelector<HTMLAnchorElement>('.md a')!.href, 'https://example.com/', 'references after the fence still resolve');
    const htmlPrefix = '<div>Note</div>\n\nSee [docs][d]\n\n';
    setText(htmlPrefix + fence(source(1)) + '\n\n[d]: https://example.com');
    setVisuals([{ ...a, versions: [{ ...a.versions[0]!, start: htmlPrefix.length, end: htmlPrefix.length + fence(source(1)).length }] }]);
    equal(host.querySelector<HTMLAnchorElement>('.md a')!.href, 'https://example.com/', 'raw HTML before the fence keeps the same reference scope');
    const detail = host.querySelector<HTMLDetailsElement>('.visual-card details')!;
    detail.open = true;
    await settle();
    ok(detail.querySelector('code')!.textContent!.includes('<title>Budget chart</title>'));
    detail.open = false;
    await settle();
    equal(detail.querySelector('pre'), null, 'closing code releases it');
    host.querySelector<HTMLButtonElement>('.visual-open')!.click();
    deepEqual(openedVisual(), { conversationId: 'visual', answerId: 'v1:0', messageId: 'v1', block: 0 });
    equal(host.querySelector('iframe'), null, 'cards never create frames');
  } finally { showVisual(undefined); dispose(); host.remove(); }
});

const look = {
  vars: {}, scheme: 'dark', motion: 'reduced', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
  appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'reduced', effects: { ambient: false, texture: false, glow: false } },
};

check('visual answers: a visual written between tool steps keeps its card when the finished reply folds into the work log', () => {
  const host = document.createElement('div'); document.body.append(host);
  const a = answer(); a.versions = [a.versions[0]!];
  const before = fence(source(1)) + '\n\n';
  const message: Message = { ...messages()[0]!, text: before + 'Done.', activityEnd: before.length, tools: [{ id: 'read', name: 'Read', input: {}, ok: true }] };
  const dispose = render(() => createComponent(Thread, { messages: [message], visuals: [a], conversationId: 'visual', options: [], from: 0, onBranch: () => {} }), host);
  try {
    ok(host.querySelector('.ai > .md .visual-card'), 'the visual card stays visible in the reply');
    ok(host.querySelector('.visual-card')!.compareDocumentPosition(host.querySelector('.steps')!) & Node.DOCUMENT_POSITION_FOLLOWING, 'the card stays before the tool step');
    equal(host.querySelector('.codeblock'), null);
    ok(host.textContent!.includes('Done.'));
  } finally { dispose(); host.remove(); }
});

check('visual files: saved file locators preserve visible words in order', () => {
  const host = document.createElement('div'); document.body.append(host);
  const a = answer(); a.versions = [{ ...a.versions[0]!, file: 'page.html', start: 0, end: 0 }];
  const before = 'Hidden first paragraph.\n\nHidden second paragraph.\n\nShown third paragraph.\n\nShown fourth paragraph.\n\n';
  const message: Message = { ...messages()[0]!, text: before + 'Done.', activityEnd: before.length, tools: [{ id: 'read', name: 'Read', input: {}, ok: true }] };
  const dispose = render(() => createComponent(Thread, { messages: [message], visuals: [a], conversationId: 'visual', options: [], from: 0, onBranch: () => {} }), host);
  try {
    const words = [...host.querySelectorAll('.ai > .md')].map((part) => part.textContent).join('');
    ok(words.includes('Hidden first paragraph.') && words.includes('Shown fourth paragraph.') && words.includes('Done.'));
    ok(words.indexOf('Hidden first paragraph.') < words.indexOf('Shown fourth paragraph.'));
    equal(host.querySelectorAll('.visual-card').length, 1, 'saved file appears once, after the reply');
  } finally { dispose(); host.remove(); }
});

check('visual answers: Chat and Projects at 720 px reuse the resizable running pane, switch versions and release the frame with reduced motion', async () => {
  const before = { ...api };
  const stored = { place: localStorage.getItem('faber.place'), layout: localStorage.getItem('faber.layout') };
  const width = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 720 });
  window.dispatchEvent(new Event('resize'));
  const revision = store.list().revision;
  store.offer({ revision: revision + 1, items: [] });
  try {
    for (const area of ['chat', 'project'] as const) for (const form of ['reply', 'file', 'mermaid'] as const) {
      useLanguage('de');
      const host = document.createElement('div');
      host.style.cssText = 'position:fixed;top:0;left:0;width:720px;height:800px';
      document.body.append(host);
      const loaded = loadedVisual(host);
      api.visualPage = (id, revision, document) => before.visualPage(id, revision, loaded.prepared(document));
      const visual = answer();
      const replies = messages();
      if (form === 'file') {
        visual.name = 'site/index.html';
        visual.versions = visual.versions.map((v) => ({ ...v, file: 'site/index.html', start: 0, end: 0 }));
        for (const message of replies) message.text = 'Files ready.';
      }
      if (form === 'mermaid') {
        visual.kind = 'mermaid';
        visual.versions = visual.versions.map((v, i) => {
          const source = `flowchart LR\nA-->B\n%% data-version="${i + 1}"`;
          const text = '```mermaid\n' + source + '\n```';
          replies[i]!.text = text;
          return { ...v, kind: 'mermaid', source, start: 0, end: text.length };
        });
      }
      const c: Conversation = { id: `visual-${area}-${form}`, title: 'Visual answers', area, projectId: area === 'project' ? 'p1' : undefined, sessions: {}, messages: replies, createdAt: 1, updatedAt: 2 };
      const initial = {
        conversations: [{ id: c.id, title: c.title, area, projectId: c.projectId, updatedAt: 2 }],
        projects: [{ id: 'p1', name: 'Visual project', path: 'E:/visual-project' }], vendors: [],
        settings: { setup: 'done', tutorialDone: true, language: 'de', appearance: { base: 'sala', motion: 'reduced' } },
        running: { revision: revision + 1, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
      } as unknown as AppState;
      api.open = async () => c; api.state = async () => initial;
      api.versions = async () => []; api.visualAnswers = async () => [visual];
      api.warm = async () => {}; api.routeDraft = async () => {}; api.setClock = async () => {}; api.setShellTexts = async () => {};
      api.changes = async () => ({ files: [], incomplete: false });
      api.readFileAt = async () => ({ path: 'chart.ts', line: 2, first: 1, text: 'first\nsecond', total: 2, tooLarge: false, binary: false });
      api.sitePlace = async () => true; api.sitePark = async () => {};
      api.onSite = async () => () => {};
      localStorage.setItem('faber.place', JSON.stringify({ area, projectId: c.projectId, id: c.id }));
      localStorage.removeItem('faber.layout');
      applyLook(initial.look, host);
      const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
      try {
        await until(() => host.querySelectorAll('.visual-card').length === 2, `${area}/${form}: two visual cards render`);
        equal(host.querySelector('iframe'), null);
        host.querySelectorAll<HTMLButtonElement>('.visual-open')[1]!.click();
        await settle();
        const pane = host.querySelector<HTMLElement>('.visual-pane')!;
        ok(pane, 'the reply opens the existing column');
        equal(host.querySelectorAll('#running').length, 1);
        equal(host.querySelector('#running')!.getAttribute('aria-modal'), 'true');
        ok(host.querySelector<HTMLElement>('#running')!.contains(document.activeElement), 'focus enters the modal column');
        equal(host.querySelector('.dock'), null, 'no connected AI keeps the composer hidden');
        ok(host.querySelector<HTMLElement>('.thread')!.inert, 'the conversation stays behind the modal');
        equal(host.querySelector('.visual-version')!.textContent, t('visual.version', { at: 2, count: 2 }));
        await until(() => host.querySelectorAll('iframe').length === 1, 'the opened version gets its frame');
        const newest = host.querySelector('iframe')!;
        const browsingContext = newest.contentWindow;
        host.querySelector<HTMLButtonElement>('.visual-prev')!.click();
        equal(host.querySelector('iframe'), newest, 'a version switch reuses the frame');
        equal(newest.contentWindow, browsingContext, 'versions keep one browsing context');
        equal(newest.isConnected, true);
        await until(() => prepared.get(loaded.url() ?? '')?.document.includes('data-version="1"'), () => `${area}/${form}: version 1 document (${loaded.why()})`);
        host.querySelector<HTMLButtonElement>('.visual-next')!.click();
        await until(() => prepared.get(loaded.url() ?? '')?.document.includes('data-version="2"'), () => `${area}/${form}: version 2 document (${loaded.why()})`);
        const grip = host.querySelector<HTMLElement>('#running [role="separator"]')!;
        const oldWidth = Number(grip.getAttribute('aria-valuenow'));
        grip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
        ok(Number(grip.getAttribute('aria-valuenow')) > oldWidth, 'the existing keyboard resize widens the pane');
        ok(JSON.parse(localStorage.getItem('faber.layout')!).panes.visual > oldWidth, 'the existing saved width is used');
        ok(pane.getBoundingClientRect().width <= 720, 'the pane fits a 720 px window');
        equal(pane.getAnimations({ subtree: true }).length, 0, 'visual controls add no motion in the reduced path');
        host.querySelector<HTMLButtonElement>('.visual-pane .back')!.click();
        await settle();
        host.querySelector<HTMLButtonElement>('.rc-commands')!.click();
        await settle();
        ok(host.querySelector('.commands-view'), 'Commands still opens in the shared pane');
        showVisual({ conversationId: c.id, answerId: 'v1:0' });
        await settle();
        equal(host.querySelector('.commands-view'), null, 'opening a visual releases the Commands view');
        await until(() => host.querySelectorAll('iframe').length === 1, 'the visual reopens in one frame after Commands');
        showFile('chart.ts:2');
        await until(() => host.querySelector('.file-line.mark'), `${area}/${form}: file mark after Commands`);
        equal(openedVisual(), undefined, 'opening a named file clears the visual selection');
        equal(host.querySelector('iframe'), null, 'opening a named file releases the visual frame');
        equal(host.querySelector('.file-line.mark')!.textContent, 'second');
        equal(host.querySelector('#running')!.getAttribute('aria-modal'), 'true');
        showVisual({ conversationId: c.id, answerId: 'v1:0' });
        await settle();
        equal(openedFile(), undefined, 'opening a visual clears the named file selection');
        equal(host.querySelector('.file-view'), null, 'opening a visual releases the file view');
        await until(() => host.querySelectorAll('iframe').length === 1, 'the visual reopens in one frame after a file');
        showVisual({ conversationId: c.id, answerId: 'pending-answer' });
        showFile('chart.ts:2');
        await until(() => host.querySelector('.file-line.mark'), `${area}/${form}: file mark after a file`);
        equal(openedVisual(), undefined, 'a named file clears even a pending visual projection');
        equal(host.querySelector('iframe'), null);
        showVisual({ conversationId: c.id, answerId: 'v1:0' });
        await settle();
        host.querySelector<HTMLButtonElement>('.visual-pane .back')!.click();
        await settle();
        showVisual({ conversationId: c.id, answerId: 'pending-answer' });
        host.querySelector<HTMLButtonElement>('.rc-commands')!.click();
        await settle();
        equal(openedVisual(), undefined, 'Commands clears a visual selection even while its projection is pending');
        equal(host.querySelector('iframe'), null);
        ok(host.querySelector('.commands-view'));
        showVisual({ conversationId: c.id, answerId: 'v1:0' });
        await settle();
        showSite({ conversationId: c.id, title: 'Example', url: 'https://example.com' });
        await settle();
        equal(host.querySelector('iframe'), null, 'the browser page excludes the visual frame');
        equal(openedVisual(), undefined);
        host.querySelector<HTMLButtonElement>('.visual-open')!.click();
        await settle();
        equal(openedSite(), undefined, 'a visual closes the browser page');
        equal(host.querySelector('.visual-version')!.textContent, t('visual.version', { at: 1, count: 2 }), 'an older card opens its own version');
        showVisual({ conversationId: c.id, answerId: 'v1:0' });
        await settle();
        equal(host.querySelector('.visual-version')!.textContent, t('visual.version', { at: 2, count: 2 }), 'unspecified version opens newest');
        host.querySelector<HTMLButtonElement>('.visual-pane .back')!.click();
        await settle();
        equal(host.querySelector('iframe'), null);
        ok(host.querySelector('.rc-body'), 'back shows the running list');
        host.querySelector<HTMLButtonElement>('.visual-open')!.click();
        await settle();
        await until(() => host.querySelector('iframe'), 'the reopened visual gets its frame');
        const closing = host.querySelector('iframe')!;
        host.querySelector<HTMLButtonElement>('.visual-pane .side-head .iconbtn:last-child')!.click();
        equal(host.querySelector('iframe'), null, 'closing immediately removes the frame');
        equal(closing.srcdoc, '', 'closing releases the last serialized document');
        equal(closing.getAttribute('srcdoc'), null, 'closing removes srcdoc so it cannot override the empty navigation');
        equal(closing.src, 'about:blank', 'closing navigates away from the opaque page');
        equal(closing.getAttribute('sandbox'), 'allow-scripts', 'the empty disposal document keeps the sandbox');
        equal(closing.contentWindow, null, 'closing destroys the browsing context');
      } finally { dispose(); host.remove(); loaded.stop(); showVisual(undefined); showSite(undefined); showFile(undefined); }
    }
  } finally {
    Object.assign(api, before);
    store.offer({ revision: revision + 2, items: [] });
    Object.defineProperty(window, 'innerWidth', width); window.dispatchEvent(new Event('resize'));
    for (const [key, value] of Object.entries(stored)) if (value === null) localStorage.removeItem(`faber.${key}`); else localStorage.setItem(`faber.${key}`, value);
  }
});

check('visual answers: one real frame navigates between versions with fresh globals and the same offline sandbox', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  let result: { version: number; fresh: boolean; blocked: boolean; policy: boolean; rtc: boolean; isolated: boolean } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  const a = answer();
  a.versions = a.versions.map((v, i) => ({ ...v, source: `<script>
    (async()=>{
      const fresh=window.previousVersion===undefined; window.previousVersion=${i + 1};
      let blocked=false,policy=false,rtc=false,isolated=false;
      document.addEventListener('securitypolicyviolation',e=>{if(e.effectiveDirective==='connect-src' && e.originalPolicy.includes("default-src 'none'"))policy=true;});
      try{await fetch('https://example.com/version-probe')}catch{blocked=true}
      try{new RTCPeerConnection()}catch{rtc=true}
      try{parent.document.body}catch{isolated=true}
      await new Promise(r=>setTimeout(r,40));
      parent.postMessage({probe:${JSON.stringify(probe)},version:${i + 1},fresh,blocked,policy,rtc,isolated},'*');
    })();
  </script>` }));
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'reuse', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => host.querySelector('iframe'));
    await until(() => host.querySelector('iframe'));
    const frame = host.querySelector('iframe')!;
    const browsingContext = frame.contentWindow;
    for (const version of [2, 1, 2]) {
      if (result) {
        result = undefined;
        host.querySelector<HTMLButtonElement>(version === 1 ? '.visual-prev' : '.visual-next')!.click();
      }
      await until(() => result?.version === version, `version ${version} starts in the reused frame`);
      equal(host.querySelector('iframe'), frame);
      equal(host.querySelectorAll('iframe').length, 1);
      equal(frame.contentWindow, browsingContext);
      equal(frame.getAttribute('sandbox'), 'allow-scripts');
      equal(prepared.get(frame.src)!.live, false);
      for (const flag of ['fresh', 'blocked', 'policy', 'rtc', 'isolated'] as const) ok(result![flag], `${flag} holds in version ${version}`);
    }
  } finally { dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: real-frame DOMContentLoaded startup reaches Ready once and wires a working click counter for classic and module scripts', async () => {
  for (const type of ['', 'module']) {
    const host = document.createElement('div'); document.body.append(host);
    const probe = crypto.randomUUID();
    let result: { status: string; count: string; dom: number; windowDom: number; loads: number; bodyLoads: number; order: string[] } | undefined;
    const listen = (e: MessageEvent) => {
      if (e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
    };
    window.addEventListener('message', listen);
    const page = `<body onload="window.bodyLoads=(window.bodyLoads||0)+1"><p id="status">Waiting</p><button id="counter">0</button>
      <script type="${type}">
        window.startupOrder.push('startup');
        let dom=0,windowDom=0,loads=0;
        document.addEventListener('DOMContentLoaded',()=>{
          dom++; document.getElementById('status').textContent='Ready';
          document.getElementById('counter').addEventListener('click',e=>e.target.textContent=String(Number(e.target.textContent)+1));
        });
        window.addEventListener('DOMContentLoaded',()=>windowDom++);
        const report=()=>parent.postMessage({probe:${JSON.stringify(probe)},status:document.getElementById('status').textContent,count:document.getElementById('counter').textContent,dom,windowDom,loads,bodyLoads:window.bodyLoads,order:window.startupOrder},'*');
        window.addEventListener('load',()=>{loads++;queueMicrotask(report)});
        window.addEventListener('message',e=>{if(e.data===${JSON.stringify(probe)}){document.getElementById('counter').click();report()}});
      </script>
      <script>window.startupOrder.push('later classic');</script></body>`;
    // A module is deferred until later classics have supplied its globals; a classic runs immediately in source order.
    const setup = '<script>window.startupOrder=[];</script>';
    const a = answer(); a.versions = [{ ...a.versions[0]!, source: setup + page }];
    const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'startup', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
    try {
      await until(() => host.querySelector('iframe'));
    const frame = host.querySelector('iframe')!;
      equal(frame.getAttribute('sandbox'), 'allow-scripts');
      equal(prepared.get(frame.src)!.live, false);
      await until(() => result, `${type || 'classic'} page delivers its startup and load report`);
      equal(result!.status, 'Ready', `${type || 'classic'} starts through its own DOMContentLoaded listener`);
      equal(result!.dom, 1); equal(result!.windowDom, 1); equal(result!.loads, 1); equal(result!.bodyLoads, 1);
      deepEqual(result!.order, type ? ['later classic', 'startup'] : ['startup', 'later classic']);
      frame.contentWindow!.postMessage(probe, '*');
      await until(() => result!.count === '1');
      frame.contentWindow!.postMessage(probe, '*');
      await until(() => result!.count === '2');
      equal(result!.dom, 1, 'clicks keep the same single initialization');
      equal(result!.loads, 1, 'the native and replayed load must not both reach the page');
    } finally { dispose(); host.remove(); window.removeEventListener('message', listen); }
  }
});

check('visual answers: real image loads reach converted resource handlers on Document before page load', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  let result: { status: string; callbacks: number; loads: number } | undefined;
  const listen = (e: MessageEvent) => {
    if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
  };
  window.addEventListener('message', listen);
  const a = answer();
  a.versions = [{ ...a.versions[0]!, source: `<p id="status">Waiting</p><script>
    window.callbacks=0;window.loads=0;
    function loadedImage(event){window.callbacks++;document.getElementById('status').textContent=event.target===this && event.currentTarget===this?'Ready':'Wrong target';}
    window.addEventListener('load',()=>{window.loads++;parent.postMessage({probe:${JSON.stringify(probe)},status:document.getElementById('status').textContent,callbacks:window.callbacks,loads:window.loads},'*');});
    </script><img src="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/%3E" onload="loadedImage.call(this,event)">` }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'resource-load', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result, 'the real frame reports its image and page load callbacks');
    equal(result!.status, 'Ready'); equal(result!.callbacks, 1); equal(result!.loads, 1);
  } finally { dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: a malformed resource handler cannot stop the real page startup', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const probe = crypto.randomUUID();
  let ready = false;
  const listen = (e: MessageEvent) => {
    if (e.origin === 'null' && e.source === host.querySelector('iframe')?.contentWindow && e.data === probe) ready = true;
  };
  window.addEventListener('message', listen);
  const a = answer();
  a.versions = [{ ...a.versions[0]!, source: `<img src="data:,bad" onerror="if ("><p id="status">Waiting</p><script>
    document.addEventListener('DOMContentLoaded',()=>{
      document.getElementById('status').textContent='Ready';
      parent.postMessage(${JSON.stringify(probe)},'*');
    });</script>` }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'malformed', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try { await until(() => ready, 'a malformed handler must not abort unrelated startup scripts'); }
  finally { dispose(); host.remove(); window.removeEventListener('message', listen); }
});

check('visual answers: the real frame runs offline, blocks fetch and Faber access, and descendants cannot restore WebRTC', async () => {
  const host = document.createElement('div'); document.body.append(host);
  let parsingRan = false;
  Object.defineProperty(window, '__visualParsingProbe', { configurable: true, set: () => { parsingRan = true; } });
  const serialized = visualDocument('<html lang="fr"><head><title>Saved</title></head><body class="chart" onload="this.dataset.ready=1"><script>parent.__visualParsingProbe=true</script><p>Saved</p></body></html>');
  equal(parsingRan, false, 'source parsing does not execute authored scripts in Faber');
  ok(serialized.includes('lang="fr"') && serialized.includes('class="chart"'), 'full document attributes survive');
  ok(serialized.indexOf('Content-Security-Policy') < serialized.indexOf('<script'), 'CSP precedes authored scripts');
  delete (window as unknown as Record<string, unknown>).__visualParsingProbe;
  const probe = crypto.randomUUID();
  const navigation = new URL(`visual-navigation-${probe}`, location.href).href;
  let navigationBlocked = false;
  const violation = (e: SecurityPolicyViolationEvent) => {
    if (e.effectiveDirective === 'frame-src' && e.originalPolicy.includes("frame-src faber-visual:") && e.blockedURI.startsWith(new URL(navigation).origin)) navigationBlocked = true;
  };
  document.addEventListener('securitypolicyviolation', violation);
  let result: Record<string, boolean> | undefined;
  const listen = (e: MessageEvent) => {
    if (e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) {
      if (e.data.navigationBlocked) navigationBlocked = true; else result = e.data;
    }
  };
  window.addEventListener('message', listen);
  const script = `<button id="counter" onclick="this.textContent=Number(this.textContent)+1">0</button><script>
    (async () => {
      let fetchBlocked=false, fetchPolicy=false, parentBlocked=false, nativeDenied=false, rtcBlocked=false, nestedBlocked=false;
      document.addEventListener('securitypolicyviolation', e => {
        if(e.effectiveDirective==='connect-src' && e.originalPolicy.includes("default-src 'none'") && e.originalPolicy.includes("script-src 'unsafe-inline'") && e.blockedURI.startsWith('https://example.com')) fetchPolicy=true;
      });
      try { await fetch('https://example.com/visual-probe'); } catch { fetchBlocked=true; }
      try { parent.document.body; } catch { parentBlocked=true; }
      try { nativeDenied=await Promise.race([window.__TAURI_INTERNALS__.invoke('state').then(()=>false,()=>true),new Promise(r=>setTimeout(()=>r(true),250))]); } catch { nativeDenied=true; }
      try { new RTCPeerConnection({iceServers:[{urls:'stun:127.0.0.1:9'}]}); } catch { rtcBlocked=true; }
      const child=document.createElement('iframe'); document.body.append(child);
      try { new child.contentWindow.RTCPeerConnection(); } catch { nestedBlocked=true; }
      child.remove();
      await new Promise(r=>setTimeout(r,40));
      document.querySelector('#counter').click();
      const interactive=document.querySelector('#counter').textContent==='1';
      parent.postMessage({probe:${JSON.stringify(probe)},fetchBlocked,fetchPolicy,parentBlocked,nativeDenied,rtcBlocked,nestedBlocked,interactive},'*');
      location.href=${JSON.stringify(navigation)};
      setTimeout(()=>parent.postMessage({probe:${JSON.stringify(probe)},navigationBlocked:true},'*'),150);
    })();
  </script>`;
  const a = answer(); a.versions = [{ ...a.versions[0]!, source: script }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'security', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => host.querySelector('iframe'));
    const frame = host.querySelector('iframe')!;
    equal(frame.getAttribute('sandbox'), 'allow-scripts');
    ok(frame.src.includes('faber-visual'), 'the page has its own protocol origin');
    equal(frame.getAttribute('srcdoc'), null);
    equal(frame.referrerPolicy, 'no-referrer');
    equal(prepared.get(frame.src)!.live, false);
    await until(() => result);
    for (const name of ['fetchBlocked', 'fetchPolicy', 'parentBlocked', 'nativeDenied', 'rtcBlocked', 'nestedBlocked', 'interactive']) equal(result![name], true, name);
    await until(() => navigationBlocked);
  } finally { dispose(); host.remove(); window.removeEventListener('message', listen); document.removeEventListener('securitypolicyviolation', violation); }
});
