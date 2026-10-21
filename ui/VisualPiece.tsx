import { createEffect, createMemo, createResource, createSignal, createUniqueId, For, Index, Match, on, onCleanup, Show, Switch } from 'solid-js';
import { api, type PieceValue, type VisualAnswer, type VisualPiece } from './api.ts';
import { lang, t } from './i18n.ts';
import { MarkdownReferences, referenceLinks } from './markdown.tsx';
import { pieceResult } from './piece-expression.ts';
import { pieceEdit, reconcilePieceEdit } from './piece-edits.ts';
import { VisualReply } from './VisualAnswer.tsx';

/** Trusted components receive only the engine's validated data; model strings are always text nodes. */
export function PieceView(props: { piece: VisualPiece; conversationId: string; revision?: number; confirmedValue?: PieceValue; refreshing?: boolean; onValue?: (value: PieceValue) => void; onDirty?: (dirty: boolean) => void }) {
  const uid = createUniqueId();
  const data = () => props.piece.data;
  const identity = createMemo(() => JSON.stringify([props.conversationId, props.piece.messageId, props.piece.block, props.piece.data]));
  const edit = createMemo(on(identity, () => pieceEdit(props.conversationId, props.piece)));
  const value = () => edit().value();
  const failed = () => edit().failed();
  const revision = createMemo(() => props.revision ?? 0);
  // A restoration is authoritative; ordinary streaming discovery may predate the last save.
  createEffect(on(revision, () => edit().reconcile(props.confirmedValue ?? props.piece.value), { defer: true }));
  createEffect(() => props.onDirty?.(edit().saving() || failed()));
  createEffect(() => props.onValue?.(value()));
  const change = (next: PieceValue) => { if (!props.refreshing) edit().change(next); };
  const index = () => typeof value() === 'number' ? value() as number : 0;
  const format = (number: number) => new Intl.NumberFormat(lang(), { maximumFractionDigits: 6 }).format(number);
  const tabsKey = (event: KeyboardEvent, count: number) => {
    const move = { ArrowLeft: -1, ArrowRight: 1 }[event.key];
    if (move === undefined && event.key !== 'Home' && event.key !== 'End') return;
    event.preventDefault();
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? count - 1 : (index() + move! + count) % count;
    change(next);
    (event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
  };
  return <section class="visual-piece" data-piece={data().type} aria-labelledby={`${uid}-title`} aria-busy={props.refreshing || undefined}>
    <h3 id={`${uid}-title`} tabIndex={-1}>{data().title}</h3>
    <Switch>
      <Match when={data().type === 'number' && data()}>{(d) => {
        const piece = d(); if (piece.type !== 'number') return;
        return <dl class="piece-numbers"><For each={piece.items}>{(item) => <div><dt>{item.label}</dt><dd>{item.value}</dd></div>}</For></dl>;
      }}</Match>
      <Match when={data().type === 'table' && data()}>{(d) => {
        const piece = d(); if (piece.type !== 'table') return;
        return <div class="piece-table" role="region" aria-label={piece.title} tabIndex={0}><table><thead><tr><For each={piece.columns}>{(column) => <th scope="col">{column}</th>}</For></tr></thead><tbody><For each={piece.rows}>{(row) => <tr><For each={row}>{(cell) => <td>{cell}</td>}</For></tr>}</For></tbody></table></div>;
      }}</Match>
      <Match when={data().type === 'chart' && data()}>{(d) => {
        const piece = d(); if (piece.type !== 'chart') return;
        const min = Math.min(0, ...piece.items.map((item) => item.value));
        const max = Math.max(0, ...piece.items.map((item) => item.value));
        const scale = (n: number) => (n - min) / (max - min || 1) * 360 + 8;
        const zero = scale(0);
        return <>
          <div class="piece-chart"><For each={piece.items}>{(item) => <div class="piece-bar"><span>{item.label}: {format(item.value)}</span><svg viewBox="0 0 376 16" aria-hidden="true"><line x1={zero} x2={zero} y1="0" y2="16" /><rect x={Math.min(zero, scale(item.value))} y="2" width={Math.max(1, Math.abs(scale(item.value) - zero))} height="12" /></svg></div>}</For></div>
          <details class="piece-data"><summary>{t('piece.data')}</summary><table><thead><tr><th scope="col">{t('piece.label')}</th><th scope="col">{t('piece.value')}</th></tr></thead><tbody><For each={piece.items}>{(item) => <tr><th scope="row">{item.label}</th><td>{format(item.value)}</td></tr>}</For></tbody></table></details>
        </>;
      }}</Match>
      <Match when={data().type === 'calculation' && data()}>{(d) => {
        const piece = d(); if (piece.type !== 'calculation') return;
        const result = () => pieceResult(piece.expression, index());
        return <div class="piece-calculation"><label for={`${uid}-range`}>{piece.label}: {format(index())}</label><input class="piece-range" id={`${uid}-range`} type="range" disabled={props.refreshing} min={piece.min} max={piece.max} step={piece.step} value={index()} onInput={(e) => change(e.currentTarget.valueAsNumber)} /><output aria-live="polite" for={`${uid}-range`}>{t('piece.result')}: {result() === undefined ? t('piece.unavailable') : format(result()!)}</output></div>;
      }}</Match>
      <Match when={data().type === 'checklist' && data()}>{(d) => {
        const piece = d(); if (piece.type !== 'checklist') return;
        return <div class="piece-choices"><For each={piece.items}>{(item, i) => <label><input type="checkbox" disabled={props.refreshing} checked={Array.isArray(value()) && (value() as boolean[])[i()]} onChange={(e) => { const next = [...value() as boolean[]]; next[i()] = e.currentTarget.checked; change(next); }} /><span>{item}</span></label>}</For></div>;
      }}</Match>
      <Match when={data().type === 'tabs' && data()}>{(d) => {
        const piece = d(); if (piece.type !== 'tabs') return;
        return <><div class="piece-tabs" role="tablist" aria-label={piece.title} onKeyDown={(e) => { if (!props.refreshing) tabsKey(e, piece.items.length); }}><For each={piece.items}>{(item, i) => <button type="button" role="tab" disabled={props.refreshing} id={`${uid}-tab-${i()}`} aria-selected={i() === index()} aria-controls={`${uid}-panel`} tabIndex={i() === index() ? 0 : -1} onClick={() => change(i())}>{item.label}</button>}</For></div><div class="piece-panel" id={`${uid}-panel`} role="tabpanel" tabIndex={0} aria-labelledby={`${uid}-tab-${index()}`}>{piece.items[index()]?.text}</div></>;
      }}</Match>
      <Match when={(data().type === 'steps' || data().type === 'form') && data()}>{(d) => {
        const piece = d(); if (piece.type !== 'steps' && piece.type !== 'form') return;
        const items = 'options' in piece ? piece.options : piece.items;
        return <><fieldset class="piece-choices" disabled={props.refreshing}><legend class="sr">{piece.title}</legend><For each={items}>{(item, i) => <label><input type="radio" name={uid} checked={index() === i()} onChange={() => change(i())} /><span>{piece.type === 'steps' ? `${i() + 1}. ` : ''}{item}</span></label>}</For></fieldset><Show when={piece.type === 'form'}><button type="button" disabled={props.refreshing} class="btn piece-fill" onClick={() => document.dispatchEvent(new CustomEvent('faber:piece-fill', { detail: { conversationId: props.conversationId, text: `${piece.title}: ${items[index()]}` } }))}>{t('piece.fill')}</button></Show></>;
      }}</Match>
      <Match when={data().type === 'notice' && data()}>{(d) => { const piece = d(); return piece.type === 'notice' ? <p class="piece-notice">{piece.text}</p> : undefined; }}</Match>
      <Match when={data().type === 'timeline' && data()}>{(d) => {
        const piece = d(); if (piece.type !== 'timeline') return;
        return <ol class="piece-timeline"><For each={piece.items}>{(item) => <li><b>{item.label}</b><p>{item.text}</p></li>}</For></ol>;
      }}</Match>
    </Switch>
    <Show when={failed()}><p class="piece-save-error" role="alert">{t('piece.saveFailed')} <button type="button" disabled={props.refreshing} class="link" onClick={() => void edit().persist()}>{t('piece.retry')}</button> <button type="button" disabled={props.refreshing} class="link piece-discard" onClick={() => edit().discard()}>{t('piece.discard')}</button></p></Show>
  </section>;
}

// Only nearby pieces keep their controls/graphs alive. Distant ones retain height and an accessible way to open them.
const near = new Map<Element, (visible: boolean) => void>();
let observer: IntersectionObserver | undefined;
function InlinePiece(props: { piece: VisualPiece; conversationId: string; revision: number; refreshing: boolean }) {
  const [visible, setVisible] = createSignal(false);
  const [height, setHeight] = createSignal(100);
  let root!: HTMLDivElement;
  const [current, setCurrent] = createSignal(props.piece);
  let dirty = false;
  let intersecting = false;
  createEffect(on(() => [props.piece.data, props.revision] as const, ([data, revision], before) => {
    const restored = revision > 0 && revision !== before?.[1] ? reconcilePieceEdit(props.conversationId, props.piece) : undefined;
    if (!before) {
      if (restored !== undefined) setCurrent((piece) => ({ ...piece, value: restored }));
      return;
    }
    if (JSON.stringify(data) !== JSON.stringify(before[0])) setCurrent(props.piece);
    else if (revision !== before[1]) setCurrent((piece) => ({ ...piece, value: restored ?? props.piece.value }));
  }));
  const refresh = () => {
    if (!intersecting && (root.contains(document.activeElement) || dirty)) return;
    if (!intersecting && visible()) setHeight(Math.max(100, root.getBoundingClientRect().height));
    setVisible(intersecting);
  };
  const observe = (element: HTMLDivElement) => {
    root = element;
    observer ??= new IntersectionObserver((entries) => { for (const entry of entries) near.get(entry.target)?.(entry.isIntersecting); }, { rootMargin: '200px' });
    near.set(root, (show) => {
      intersecting = show;
      refresh();
    });
    observer.observe(root);
  };
  onCleanup(() => { observer?.unobserve(root); near.delete(root); if (!near.size) { observer?.disconnect(); observer = undefined; } });
  return <div ref={observe} class="piece-slot" style={{ 'min-height': `${height()}px` }} onFocusOut={() => queueMicrotask(refresh)}>
    <Show when={visible()} fallback={<button type="button" class="piece-placeholder" onClick={() => { setVisible(true); queueMicrotask(() => root.querySelector<HTMLElement>('h3')?.focus()); }}>{t('piece.show', { title: props.piece.data.title })}</button>}>
      <PieceView piece={current()} conversationId={props.conversationId} revision={props.revision} confirmedValue={props.piece.value} refreshing={props.refreshing} onValue={(value) => setCurrent((piece) => ({ ...piece, value }))} onDirty={(value) => { const wasDirty = dirty; dirty = value; if (wasDirty && !value) queueMicrotask(refresh); }} />
    </Show>
  </div>;
}

/** Split at engine-validated UTF-16 ranges. Remaining blocks retain the existing visual pane and Markdown behavior. */
export function PieceReply(props: { text: string; offset: number; messageId: string; conversationId: string; answers: VisualAnswer[]; revision?: number }) {
  return <Show when={JSON.stringify([props.conversationId, props.messageId])} keyed>{(identity) => {
    const [conversationId, messageId] = JSON.parse(identity) as [string, string];
    return <PieceMessage {...props} conversationId={conversationId} messageId={messageId} />;
  }}</Show>;
}

function PieceMessage(props: { text: string; offset: number; messageId: string; conversationId: string; answers: VisualAnswer[]; revision?: number }) {
  // Streaming words between fences do not reread history. A closing fence refreshes discovery once.
  const key = createMemo(() => {
    if (!props.text.includes('faber-piece')) return;
    const fences = [...props.text.matchAll(/^ {0,3}(?:`{3,}|~{3,})[ \t]*\r?$/gm)];
    const last = fences.at(-1);
    return last ? JSON.stringify([props.conversationId, props.messageId, props.text.slice(0, last.index + last[0].length), props.revision ?? 0]) : undefined;
  });
  const [pieces] = createResource(key, async (request) => {
    const [conversationId, messageId, , revision] = JSON.parse(request) as [string, string, string, number];
    const text = props.text;
    const offset = props.offset;
    return { conversationId, messageId, text, offset, revision, pieces: await api.visualPieces(conversationId, messageId).catch(() => []) };
  });
  const parts = createMemo(() => {
    const discovered = pieces();
    const found = key() && discovered?.conversationId === props.conversationId && discovered.messageId === props.messageId
      ? discovered.pieces.filter((p) => p.messageId === props.messageId && p.start >= props.offset && p.end <= props.offset + props.text.length
        && discovered.text.slice(p.start - discovered.offset, p.end - discovered.offset) === props.text.slice(p.start - props.offset, p.end - props.offset))
      : [];
    const result: { text: string; offset: number; piece?: VisualPiece }[] = [];
    let at = 0;
    for (const piece of found) {
      const start = piece.start - props.offset;
      const end = piece.end - props.offset;
      if (start < at) continue;
      if (start > at) result.push({ text: props.text.slice(at, start), offset: at + props.offset });
      result.push({ text: props.text.slice(start, end), offset: start + props.offset, piece });
      at = end;
    }
    if (at < props.text.length) result.push({ text: props.text.slice(at), offset: at + props.offset });
    return result;
  });
  return <MarkdownReferences.Provider value={createMemo(() => referenceLinks(props.text))}><Index each={parts()}>{(part) => <Show when={part().piece ? `${part().piece!.messageId}:${part().piece!.block}` : undefined} keyed fallback={<VisualReply text={part().text} offset={part().offset} messageId={props.messageId} conversationId={props.conversationId} answers={props.answers} />}>
    {(_identity) => <InlinePiece piece={part().piece!} conversationId={props.conversationId} revision={pieces()?.revision ?? 0} refreshing={(pieces()?.revision ?? 0) !== (props.revision ?? 0)} />}
  </Show>}</Index></MarkdownReferences.Provider>;
}
