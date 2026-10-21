import { createEffect, createMemo, createSignal, Index, on, onCleanup, Show } from 'solid-js';
import type { VisualAnswer, VisualVersion } from './api.ts';
import { closeMenu } from './contextmenu.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { Markdown, referenceLinks } from './markdown.tsx';
import runtime from './visual-runtime.js?raw';

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
export const VISUAL_CSP = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; base-uri 'none'; form-action 'none'; frame-src 'none'; object-src 'none'";

export function visualDocument(source: string): string {
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
  const get = Element.prototype.getAttribute;
  const set = Element.prototype.setAttribute;
  const remove = Element.prototype.removeAttribute;
  const append = Node.prototype.appendChild;
  const query = (root: Document | DocumentFragment) => root instanceof Document
    ? Document.prototype.querySelectorAll.call(root, '*') : DocumentFragment.prototype.querySelectorAll.call(root, '*');
  const clean = (root: Document | DocumentFragment) => {
    for (const el of query(root)) {
      const tag = name.call(el) as string;
      if (['link', 'iframe', 'frame', 'object', 'embed'].includes(tag) || (tag === 'script' && get.call(el, 'type')?.trim().toLowerCase() === 'speculationrules')) {
        Node.prototype.removeChild.call(el.parentNode!, el);
        continue;
      }
      remove.call(el, 'nonce');
      remove.call(el, 'data-faber-script');
      if (tag === 'template') clean(Object.getOwnPropertyDescriptor(HTMLTemplateElement.prototype, 'content')!.get!.call(el));
    }
  };
  clean(doc);
  for (const script of Document.prototype.querySelectorAll.call(doc, 'script')) {
    if (get.call(script, 'src') !== null) { Node.prototype.removeChild.call(script.parentNode!, script); continue; }
    const type = get.call(script, 'type')?.trim().toLowerCase() ?? '';
    if (!['', 'module', 'text/javascript', 'application/javascript'].includes(type)) continue;
    set.call(script, 'data-faber-script', type || 'text/javascript');
    set.call(script, 'type', 'application/faber-script');
  }
  // The inherited hash policy also covers event attributes. Keep their normal behavior through confined listeners.
  // shortcut: event attributes created later stay blocked; generated apps must use addEventListener until a scoped policy can support them.
  const listeners: string[] = [];
  for (const [index, el] of Document.prototype.querySelectorAll.call(doc, '*').entries()) {
    remove.call(el, 'data-faber-event');
    for (const attr of Array.from(attrs.call(el) as NamedNodeMap)) {
      if (!/^on[a-z]+$/.test(attr.name)) continue;
      remove.call(el, attr.name);
      set.call(el, 'data-faber-event', String(index));
      const event = attr.name.slice(2);
      const target = el === body && /^(?:load|error|resize|scroll|unload|beforeunload|hashchange|popstate|pageshow|pagehide|message|online|offline)$/.test(event)
        ? 'window' : `document.querySelector('[data-faber-event="${index}"]')`;
      listeners.push(`${target}.addEventListener(${JSON.stringify(event)}, function(event) { if ((function(event) { ${attr.value}\n }).call(this,event) === false) event.preventDefault(); });`);
    }
  }
  if (listeners.length) {
    const script = document.createElement('script');
    set.call(script, 'type', 'application/faber-script');
    set.call(script, 'data-faber-script', 'text/javascript');
    script.textContent = listeners.join('\n').replace(/<\/script/gi, '<\\/script');
    append.call(body, script);
  }
  const policy = Document.prototype.createElement.call(doc, 'meta');
  set.call(policy, 'http-equiv', 'Content-Security-Policy');
  set.call(policy, 'content', VISUAL_CSP);
  Node.prototype.insertBefore.call(head, policy, head.firstChild);
  const loader = Document.prototype.createElement.call(doc, 'script');
  loader.textContent = runtime.replace(/\r\n?/g, '\n');
  Node.prototype.insertBefore.call(head, loader, policy.nextSibling);
  return `<!doctype html>${Object.getOwnPropertyDescriptor(Element.prototype, 'outerHTML')!.get!.call(root)}`;
}

/** One frame navigates between versions; the column's lifecycle releases it on back, close or navigation. */
export function VisualPane(props: { answer: VisualAnswer; open: OpenVisual; onBack: () => void; onClose: () => void }) {
  let frame: HTMLIFrameElement | undefined;
  const fromFrame = (e: MessageEvent) => {
    if (!frame?.isConnected || e.source !== frame.contentWindow || e.origin !== 'null' || !e.data || typeof e.data !== 'object' || Array.isArray(e.data)) return;
    if (Object.keys(e.data).length !== 1 || !Object.hasOwn(e.data, 'type')) return;
    if (e.data.type === 'faber-visual-escape') props.onClose();
    else if (e.data.type === 'faber-visual-contextmenu') closeMenu();
  };
  window.addEventListener('message', fromFrame);
  onCleanup(() => {
    window.removeEventListener('message', fromFrame);
    const retiring = frame;
    frame = undefined;
    if (retiring) {
      // srcdoc takes precedence over src: remove it to leave the opaque document before detaching the frame.
      retiring.src = 'about:blank';
      retiring.removeAttribute('srcdoc');
      retiring.remove();
    }
  });
  const initial = () => props.answer.versions.findIndex((v) => v.messageId === props.open.messageId && v.block === props.open.block);
  const [picked, setPicked] = createSignal(initial() < 0 ? props.answer.versions.length - 1 : initial());
  createEffect(on(() => props.open, () => setPicked(initial() < 0 ? props.answer.versions.length - 1 : initial()), { defer: true }));
  const at = () => Math.min(picked(), props.answer.versions.length - 1);
  const version = createMemo(() => props.answer.versions[at()]!, undefined, { equals: (a, b) => a?.messageId === b.messageId && a?.block === b.block && a?.source === b.source });
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
      <span class="side-meta">{t('visual.offline')}</span>
    </div>
    <iframe ref={(el) => (frame = el)} class="visual-frame" title={visualName(props.answer)} sandbox="allow-scripts" referrerPolicy="no-referrer" srcdoc={visualDocument(version().source)} />
  </div>;
}
