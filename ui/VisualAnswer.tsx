import { createEffect, createMemo, createResource, createSignal, Index, on, onCleanup, Show } from 'solid-js';
import { api, type VisualAnswer, type VisualVersion } from './api.ts';
import { closeMenu } from './contextmenu.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { Markdown, referenceLinks } from './markdown.tsx';
import runtime from './visual-page-runtime.js?raw';

export interface OpenVisual {
  conversationId: string;
  answerId: string;
  messageId?: string;
  block?: number;
}
const [openedVisual, showVisual] = createSignal<OpenVisual>();
export { openedVisual, showVisual };

export const visualName = (answer: VisualAnswer) => answer.name || t(answer.kind === 'svg' ? 'visual.drawing' : 'visual.page');

/** The engine supplies the closed fences' ranges; all remaining words use the reply's existing Markdown view. */
export function VisualReply(props: { text: string; offset: number; messageId: string; conversationId: string; answers: VisualAnswer[] }) {
  const links = createMemo(() => referenceLinks(props.text));
  const pieces = createMemo(() => {
    const versions = props.answers.flatMap((answer) => answer.versions
      .filter((v) => v.messageId === props.messageId && v.start >= props.offset && v.end <= props.offset + props.text.length)
      .map((version) => ({ answer, version }))).sort((a, b) => a.version.start - b.version.start);
    const out: { text: string; answer?: VisualAnswer; version?: VisualVersion }[] = [];
    let at = 0;
    for (const { answer, version } of versions) {
      const start = version.start - props.offset;
      const end = version.end - props.offset;
      if (start < at) continue;
      if (start > at) out.push({ text: props.text.slice(at, start) });
      out.push({ text: props.text.slice(start, end), answer, version });
      at = end;
    }
    if (at < props.text.length) out.push({ text: props.text.slice(at) });
    return out;
  });
  return <Index each={pieces()}>{(piece) => <Show when={piece().answer} fallback={<Markdown text={piece().text} links={links()} />}>
    {(answer) => {
      const [code, setCode] = createSignal(false);
      return <div class="visual-card">
        <div class="visual-card-head">
          <Icon name="sheet" />
          <span><b>{visualName(answer())}</b><small>{t(piece().version!.kind === 'svg' ? 'visual.drawing' : 'visual.page')}</small></span>
          <button class="btn visual-open" onClick={() => showVisual({ conversationId: props.conversationId, answerId: answer().id, messageId: props.messageId, block: piece().version!.block })}>{t('visual.open')}</button>
        </div>
        <details onToggle={(e) => setCode(e.currentTarget.open)}>
          <summary>{t('visual.code')}</summary>
          <Show when={code()}><Markdown text={piece().text} /></Show>
        </details>
      </div>;
    }}
  </Show>}</Index>;
}

// The opaque sandbox cannot read Faber; its CSP and the native origin check deny requests and commands.
export const VISUAL_CSP = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; base-uri 'none'; form-action 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'";
export const VISUAL_LIVE_CSP = "default-src 'none'; connect-src https:; script-src 'unsafe-inline' https:; style-src 'unsafe-inline' https:; img-src data: blob: https:; font-src data: https:; media-src data: blob: https:; base-uri 'none'; form-action 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'";

export function visualDocument(source: string, live = false): string {
  // This detached document has no browsing context: scripts and resource loads stay inactive, including during parsing.
  const doc = document.implementation.createHTMLDocument('');
  Reflect.apply(Document.prototype.open, doc, []);
  Document.prototype.write.call(doc, source);
  Document.prototype.close.call(doc);
  // Named forms and controls shadow document and element properties. Never dispatch through authored objects.
  const head = Object.getOwnPropertyDescriptor(Document.prototype, 'head')!.get!.call(doc) as HTMLHeadElement;
  const body = Object.getOwnPropertyDescriptor(Document.prototype, 'body')!.get!.call(doc) as HTMLElement;
  const root = Object.getOwnPropertyDescriptor(Document.prototype, 'documentElement')!.get!.call(doc) as HTMLElement;
  const attrs = Object.getOwnPropertyDescriptor(Element.prototype, 'attributes')!.get!;
  const name = Object.getOwnPropertyDescriptor(Element.prototype, 'localName')!.get!;
  const parent = Object.getOwnPropertyDescriptor(Node.prototype, 'parentNode')!.get!;
  const get = Element.prototype.getAttribute;
  const set = Element.prototype.setAttribute;
  const remove = Element.prototype.removeAttribute;
  const query = (root: Document | DocumentFragment) => root instanceof Document
    ? Document.prototype.querySelectorAll.call(root, '*') : DocumentFragment.prototype.querySelectorAll.call(root, '*');
  const clean = (root: Document | DocumentFragment) => {
    for (const el of query(root)) {
      const tag = name.call(el) as string;
      if ((tag === 'link' && (!live || get.call(el, 'rel')?.trim().toLowerCase() !== 'stylesheet'))
        || (tag === 'meta' && get.call(el, 'http-equiv')?.trim().toLowerCase() === 'refresh')
        || ['iframe', 'frame', 'object', 'embed', 'noscript'].includes(tag) || (tag === 'script' && get.call(el, 'type')?.trim().toLowerCase() === 'speculationrules')) {
        // noscript changes parsing when scripting becomes enabled in the real frame.
        Node.prototype.removeChild.call(parent.call(el), el);
        continue;
      }
      remove.call(el, 'nonce');
      remove.call(el, 'data-faber-script');
      remove.call(el, 'data-faber-src');
      remove.call(el, 'data-faber-handlers');
      remove.call(el, 'data-faber-events');
      remove.call(el, 'data-faber-order');
      if (tag === 'template') {
        // The real parser must not hide a second browsing realm in an unobservable closed shadow tree.
        remove.call(el, 'shadowrootmode');
        remove.call(el, 'shadowroot');
        clean(Object.getOwnPropertyDescriptor(HTMLTemplateElement.prototype, 'content')!.get!.call(el));
      }
    }
  };
  clean(doc);
  for (const script of Document.prototype.querySelectorAll.call(doc, 'script')) {
    const src = get.call(script, 'src');
    if (src !== null && (!live || !/^https:\/\//i.test(src.trim()))) { Node.prototype.removeChild.call(script.parentNode!, script); continue; }
    const type = get.call(script, 'type')?.trim().toLowerCase() ?? '';
    if (!['', 'module', 'text/javascript', 'application/javascript'].includes(type)) continue;
    set.call(script, 'data-faber-script', type || 'text/javascript');
    set.call(script, 'type', 'application/faber-script');
    if (src !== null) {
      set.call(script, 'data-faber-src', src);
      remove.call(script, 'src');
    }
  }
  // The response's hash policy also covers event attributes. Keep their behavior through confined listeners.
  // shortcut: event attributes created later stay blocked; generated apps must use addEventListener until a scoped policy can support them.
  const listeners: string[] = [];
  const resources = new Map<string, { elements: string[]; window?: string }>();
  let classics = 0;
  for (const [index, el] of Document.prototype.querySelectorAll.call(doc, '*').entries()) {
    remove.call(el, 'data-faber-event');
    const scriptListeners: string[] = [];
    for (const attr of Array.from(attrs.call(el) as NamedNodeMap)) {
      if (!/^on[a-z]+$/.test(attr.name)) continue;
      remove.call(el, attr.name);
      set.call(el, 'data-faber-event', String(index));
      const event = attr.name.slice(2);
      const target = el === body && /^(?:load|error|resize|scroll|unload|beforeunload|hashchange|popstate|pageshow|pagehide|message|online|offline)$/.test(event)
        ? 'window' : `document.querySelector('[data-faber-event="${index}"]')`;
      const callback = `function(event) { if ((function(event) { ${attr.value}\n }).call(this,event) === false) event.preventDefault(); }`;
      const listener = `.addEventListener(${JSON.stringify(event)}, ${callback});`;
      if (get.call(el, 'data-faber-script') !== null) scriptListeners.push(`target${listener}`);
      else if (event === 'load' || event === 'error') {
        const handlers = resources.get(event) ?? { elements: [] };
        if (target === 'window') handlers.window = callback;
        else handlers.elements.push(`[${JSON.stringify(String(index))}, { after: ${classics}, run: ${callback} }]`);
        resources.set(event, handlers);
      }
      else listeners.push(`${target}${listener}`);
    }
    if (scriptListeners.length) set.call(el, 'data-faber-handlers', scriptListeners.join('\n'));
    const scriptType = get.call(el, 'data-faber-script');
    if (scriptType !== null && scriptType !== 'module') set.call(el, 'data-faber-order', String(++classics));
  }
  if (listeners.length) {
    const script = document.createElement('script');
    set.call(script, 'type', 'application/faber-script');
    set.call(script, 'data-faber-script', 'text/javascript');
    script.textContent = listeners.join('\n').replace(/<\/script/gi, '<\\/script');
    Node.prototype.insertBefore.call(head, script, head.firstChild);
  }
  const policy = Document.prototype.createElement.call(doc, 'meta');
  set.call(policy, 'http-equiv', 'Content-Security-Policy');
  set.call(policy, 'content', live ? VISUAL_LIVE_CSP : VISUAL_CSP);
  Node.prototype.insertBefore.call(head, policy, head.firstChild);
  let early: HTMLScriptElement | undefined;
  if (resources.size) {
    early = Document.prototype.createElement.call(doc, 'script') as HTMLScriptElement;
    set.call(early, 'type', 'application/faber-events');
    set.call(early, 'data-faber-events', '');
    // Capture non-bubbling resource events before parsing their elements, without delaying their requests.
    early.textContent = `(() => {
      let completed = -1, waiting = [];
      const report = window.setTimeout.bind(window);
      const define = Object.defineProperty, describe = Object.getOwnPropertyDescriptor, remove = Reflect.deleteProperty;
      const deliver = (item) => {
        if (item.handler.after > completed) { waiting.push(item); return; }
        const previous = ['target', 'currentTarget'].map((key) => [key, describe(item.event, key)]);
        try {
          // A queued event's native target can change after dispatch; use the element captured with the handler.
          define(item.event, 'target', { value: item.target, configurable: true });
          define(item.event, 'currentTarget', { value: item.target, configurable: true });
          item.handler.run.call(item.target, item.event);
        }
        catch (error) { report(() => { throw error; }, 0); }
        finally {
          try {
            for (const [key, entry] of previous) {
              if (entry) define(item.event, key, entry);
              else remove(item.event, key);
            }
          } catch (error) { report(() => { throw error; }, 0); }
        }
      };
      Object.getOwnPropertyDescriptor(Document.prototype, 'currentScript').get.call(document).faberAdvance = (order) => {
        completed = order;
        const pending = waiting; waiting = [];
        for (const item of pending) deliver(item);
      };
      ${[...resources].map(([event, handlers]) => `(() => {
      const handlers = new Map([${handlers.elements.join(',')}]);
      const get = Element.prototype.getAttribute, lookup = Map.prototype.get;
      const at = Object.getOwnPropertyDescriptor(Event.prototype, 'target').get;
      const page = ${handlers.window ? `{ after: 0, run: ${handlers.window} }` : 'undefined'};
      // Element load events stop at Document; they never reach Window, even during capture.
      document.addEventListener(${JSON.stringify(event)}, function(event) {
        const target = at.call(event);
        const handler = target instanceof Element ? lookup.call(handlers, get.call(target, 'data-faber-event')) : undefined;
        if (handler) deliver({ handler, target, event });
      }, true);
      if (page) window.addEventListener(${JSON.stringify(event)}, function(event) {
        const target = at.call(event);
        if (target === window || target === document) deliver({ handler: page, target: window, event });
      }, true);
    })();`).join('\n')}
    })();`.replace(/<\/script/gi, '<\\/script');
    Node.prototype.insertBefore.call(head, early, policy.nextSibling);
  }
  const loader = Document.prototype.createElement.call(doc, 'script');
  loader.textContent = runtime.replace(/\r\n?/g, '\n');
  Node.prototype.insertBefore.call(head, loader, early ? early.nextSibling : policy.nextSibling);
  return `<!doctype html>${Object.getOwnPropertyDescriptor(Element.prototype, 'outerHTML')!.get!.call(root)}`;
}

/** One frame navigates between versions; the column's lifecycle releases it on back, close or navigation. */
export function VisualPane(props: { answer: VisualAnswer; open: OpenVisual; onBack: () => void; onClose: () => void }) {
  let frame: HTMLIFrameElement | undefined;
  const retire = () => {
    const old = frame; frame = undefined;
    if (old) { old.src = 'about:blank'; old.remove(); }
  };
  let disposed = false;
  let revision = 0;
  const lease = api.visualOpen();
  const fromFrame = (e: MessageEvent) => {
    if (!frame?.isConnected || e.source !== frame.contentWindow || e.origin !== 'null' || !e.data || typeof e.data !== 'object' || Array.isArray(e.data)) return;
    if (Object.keys(e.data).length !== 1 || !Object.hasOwn(e.data, 'type')) return;
    if (e.data.type === 'faber-visual-escape') props.onClose();
    else if (e.data.type === 'faber-visual-contextmenu') closeMenu();
  };
  window.addEventListener('message', fromFrame);
  onCleanup(() => {
    disposed = true;
    void lease.then((id) => api.visualClose(id)).catch(() => {});
    window.removeEventListener('message', fromFrame);
    retire();
  });
  const initial = () => props.answer.versions.findIndex((v) => v.messageId === props.open.messageId && v.block === props.open.block);
  const [picked, setPicked] = createSignal(initial() < 0 ? props.answer.versions.length - 1 : initial());
  createEffect(on(() => props.open, () => setPicked(initial() < 0 ? props.answer.versions.length - 1 : initial()), { defer: true }));
  const at = () => Math.min(picked(), props.answer.versions.length - 1);
  const version = createMemo(() => props.answer.versions[at()]!, undefined, { equals: (a, b) => a?.messageId === b.messageId && a?.block === b.block && a?.source === b.source });
  // Snapshot the saved permission only on opening/switching; changing Settings never discards a page's input.
  const [page] = createResource(() => ({ version: version(), open: props.open }), async ({ version }) => {
    const requested = ++revision;
    const id = await lease;
    if (disposed) return undefined;
    // The native response headers choose offline/live from saved Settings; the screen cannot grant it.
    return api.visualPage(id, requested, visualDocument(version.source, true));
  });
  createEffect(() => { if (page.error) retire(); });
  const move = (by: number) => setPicked(Math.max(0, Math.min(at() + by, props.answer.versions.length - 1)));
  return <div class="visual-pane">
    <div class="rc-head side-head">
      <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={props.onBack}><Icon name="left" /></button>
      <h2 id="running-title" class="display" title={visualName(props.answer)}>{visualName(props.answer)}</h2>
      <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={props.onClose}><Icon name="close" /></button>
    </div>
    <div class="visual-toolbar" role="group" aria-label={t('visual.versions')} onKeyDown={(e) => {
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
      e.preventDefault(); move(e.key === 'ArrowLeft' ? -1 : 1);
    }}>
      <button class="iconbtn visual-prev" aria-label={t('visual.previous')} title={t('visual.previous')} disabled={at() === 0} onClick={() => move(-1)}><Icon name="left" /></button>
      <span class="visual-version num" aria-live="polite">{t('visual.version', { at: at() + 1, count: props.answer.versions.length })}</span>
      <button class="iconbtn visual-next" aria-label={t('visual.next')} title={t('visual.next')} disabled={at() === props.answer.versions.length - 1} onClick={() => move(1)}><Icon name="right" /></button>
      <span class="side-meta">{t(!page.error && page()?.live ? 'visual.live' : 'visual.offline')}</span>
    </div>
    <Show when={!page.error} fallback={<p class="side-meta" role="alert">{t('visual.failed')}</p>}>
      <Show when={page()}><iframe ref={(el) => (frame = el)} class="visual-frame" title={visualName(props.answer)} sandbox="allow-scripts" referrerPolicy="no-referrer" src={page()!.url} /></Show>
    </Show>
  </div>;
}
