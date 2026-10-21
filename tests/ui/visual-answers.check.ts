import { createComponent, createSignal, Show } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppState, type Conversation, type Message, type VisualAnswer } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { openedVisual, showVisual, VisualPane, visualDocument, VISUAL_CSP } from '../../ui/VisualAnswer.tsx';
import { openedSite, showSite } from '../../ui/Site.tsx';
import { openedFile, showFile } from '../../ui/fileref.ts';
import { store } from '../../ui/running.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { menu, openMenu, closeMenu } from '../../ui/contextmenu.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
async function until(test: () => unknown, what = 'the visual answer appeared before the deadline') {
  const end = performance.now() + 4000;
  while (!test() && performance.now() < end) await settle();
  ok(test(), what);
}
const source = (n: number) => `<!doctype html><html lang="en"><head><title>Budget chart</title></head><body data-version="${n}" style="margin:0"><p>Version ${n}</p><button onclick="this.textContent='Changed'">Change</button></body></html>`;
const fence = (text: string) => `\`\`\`html\n${text}\n\`\`\``;
const answer = (): VisualAnswer => ({ id: 'v1:0', name: 'Budget chart', kind: 'html', versions: [1, 2].map((n) => ({ messageId: `v${n}`, block: 0, start: 0, end: fence(source(n)).length, kind: 'html', source: source(n) })) });
const messages = (): Message[] => [1, 2].map((n) => ({ id: `v${n}`, role: 'assistant', text: fence(source(n)), vendor: n === 1 ? 'claude' : 'codex', at: n, status: 'done' }));

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
  const a = answer(); a.versions = [{ ...a.versions[0]!, source: script }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'hints', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    await until(() => result);
    equal(result!.error, undefined, result!.error);
    equal(result!.cases.length, 23, 'every insertion path is checked synchronously, before an observer could remove a connected hint');
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
    ok(host.querySelector('.visual-card')?.closest('details'), 'the work log keeps the visual card instead of raw code');
    equal(host.querySelector('.codeblock'), null);
    ok(host.textContent!.includes('Done.'));
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
    for (const area of ['chat', 'project'] as const) {
      useLanguage('de');
      const host = document.createElement('div');
      host.style.cssText = 'position:fixed;top:0;left:0;width:720px;height:800px';
      document.body.append(host);
      const c: Conversation = { id: `visual-${area}`, title: 'Visual answers', area, projectId: area === 'project' ? 'p1' : undefined, sessions: {}, messages: messages(), createdAt: 1, updatedAt: 2 };
      const initial = {
        conversations: [{ id: c.id, title: c.title, area, projectId: c.projectId, updatedAt: 2 }],
        projects: [{ id: 'p1', name: 'Visual project', path: 'E:/visual-project' }], vendors: [],
        settings: { setup: 'done', tutorialDone: true, language: 'de', appearance: { base: 'sala', motion: 'reduced' } },
        running: { revision: revision + 1, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
      } as unknown as AppState;
      api.open = async () => c; api.state = async () => initial;
      api.versions = async () => []; api.visualAnswers = async () => [answer()];
      api.warm = async () => {}; api.routeDraft = async () => {}; api.setClock = async () => {}; api.setShellTexts = async () => {};
      api.changes = async () => ({ files: [], incomplete: false });
      api.readFileAt = async () => ({ path: 'chart.ts', line: 2, first: 1, text: 'first\nsecond', total: 2, tooLarge: false, binary: false });
      api.sitePlace = async () => true; api.sitePark = async () => {};
      api.onSite = async () => () => {};
      localStorage.setItem('faber.place', JSON.stringify({ area, projectId: c.projectId, id: c.id }));
      localStorage.removeItem('faber.layout');
      const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
      try {
        await until(() => host.querySelectorAll('.visual-card').length === 2);
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
        equal(host.querySelectorAll('iframe').length, 1);
        const newest = host.querySelector('iframe')!;
        const browsingContext = newest.contentWindow;
        host.querySelector<HTMLButtonElement>('.visual-prev')!.click();
        equal(host.querySelector('iframe'), newest, 'a version switch reuses the frame');
        equal(newest.contentWindow, browsingContext, 'versions keep one browsing context');
        equal(newest.isConnected, true);
        ok(host.querySelector('iframe')!.srcdoc.includes('data-version="1"'));
        host.querySelector<HTMLButtonElement>('.visual-next')!.click();
        ok(host.querySelector('iframe')!.srcdoc.includes('data-version="2"'));
        const grip = host.querySelector<HTMLElement>('#running [role="separator"]')!;
        const oldWidth = Number(grip.getAttribute('aria-valuenow'));
        grip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
        ok(Number(grip.getAttribute('aria-valuenow')) > oldWidth, 'the existing keyboard resize widens the pane');
        ok(JSON.parse(localStorage.getItem('faber.layout')!).run > oldWidth, 'the existing saved width is used');
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
        equal(host.querySelectorAll('iframe').length, 1);
        showFile('chart.ts:2');
        await until(() => host.querySelector('.file-line.mark'));
        equal(openedVisual(), undefined, 'opening a named file clears the visual selection');
        equal(host.querySelector('iframe'), null, 'opening a named file releases the visual frame');
        equal(host.querySelector('.file-line.mark')!.textContent, 'second');
        equal(host.querySelector('#running')!.getAttribute('aria-modal'), 'true');
        showVisual({ conversationId: c.id, answerId: 'v1:0' });
        await settle();
        equal(openedFile(), undefined, 'opening a visual clears the named file selection');
        equal(host.querySelector('.file-view'), null, 'opening a visual releases the file view');
        equal(host.querySelectorAll('iframe').length, 1);
        showVisual({ conversationId: c.id, answerId: 'pending-answer' });
        showFile('chart.ts:2');
        await until(() => host.querySelector('.file-line.mark'));
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
        const closing = host.querySelector('iframe')!;
        host.querySelector<HTMLButtonElement>('.visual-pane .side-head .iconbtn:last-child')!.click();
        equal(host.querySelector('iframe'), null, 'closing immediately removes the frame');
        equal(closing.srcdoc, '', 'closing releases the last serialized document');
        equal(closing.getAttribute('srcdoc'), null, 'closing removes srcdoc so it cannot override the empty navigation');
        equal(closing.src, 'about:blank', 'closing navigates away from the opaque page');
        equal(closing.getAttribute('sandbox'), 'allow-scripts', 'the empty disposal document keeps the sandbox');
        equal(closing.contentWindow, null, 'closing destroys the browsing context');
      } finally { dispose(); host.remove(); showVisual(undefined); showSite(undefined); showFile(undefined); }
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
      ok(frame.srcdoc.includes(VISUAL_CSP));
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
      const frame = host.querySelector('iframe')!;
      equal(frame.getAttribute('sandbox'), 'allow-scripts');
      ok(frame.srcdoc.includes(VISUAL_CSP));
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
    if (e.effectiveDirective === 'frame-src' && e.originalPolicy.includes("frame-src 'none'") && e.blockedURI.startsWith(new URL(navigation).origin)) navigationBlocked = true;
  };
  document.addEventListener('securitypolicyviolation', violation);
  let result: Record<string, boolean> | undefined;
  const listen = (e: MessageEvent) => {
    if (e.source === host.querySelector('iframe')?.contentWindow && e.data?.probe === probe) result = e.data;
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
    })();
  </script>`;
  const a = answer(); a.versions = [{ ...a.versions[0]!, source: script }];
  const dispose = render(() => createComponent(VisualPane, { answer: a, open: { conversationId: 'security', answerId: a.id }, onBack: () => {}, onClose: () => {} }), host);
  try {
    const frame = host.querySelector('iframe')!;
    equal(frame.getAttribute('sandbox'), 'allow-scripts');
    equal(frame.getAttribute('src'), null);
    equal(frame.referrerPolicy, 'no-referrer');
    ok(frame.srcdoc.includes(VISUAL_CSP));
    await until(() => result);
    for (const name of ['fetchBlocked', 'fetchPolicy', 'parentBlocked', 'nativeDenied', 'rtcBlocked', 'nestedBlocked', 'interactive']) equal(result![name], true, name);
    await until(() => navigationBlocked);
  } finally { dispose(); host.remove(); window.removeEventListener('message', listen); document.removeEventListener('securitypolicyviolation', violation); }
});
