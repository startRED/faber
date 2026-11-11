import type { Message } from './api.ts';
import { runsOf } from './Run.tsx';

export type Reading = { id: string; index: number; offset: number; run?: boolean };

/** Positions contain no message text and cannot grow with the conversation archive. */
export function savedReading(value: unknown): Map<string, Reading> {
  const positions = new Map<string, Reading>();
  if (!Array.isArray(value)) return positions;
  for (const entry of value.slice(-100)) {
    if (!Array.isArray(entry) || entry.length !== 2) continue;
    const [id, point] = entry;
    if (typeof id !== 'string' || id.length > 256 || !point || typeof point !== 'object') continue;
    const p = point as Partial<Reading>;
    if (typeof p.id !== 'string' || !p.id || p.id.length > 256 || !Number.isSafeInteger(p.index) || p.index! < 0 ||
        typeof p.offset !== 'number' || !Number.isFinite(p.offset) || Math.abs(p.offset) > 10_000_000) continue;
    positions.set(id, { id: p.id, index: p.index!, offset: p.offset, ...(p.run === true ? { run: true } : {}) });
  }
  return positions;
}

const ANCHORS = '[data-message-index], [data-reading-id]';
const nodes = (thread: HTMLElement) => [...thread.querySelectorAll<HTMLElement>(ANCHORS)];
const indexOf = (node: HTMLElement) => Number(node.dataset.readingIndex ?? node.dataset.messageIndex);
const idOf = (node: HTMLElement, messages: Message[], earlier: number) => node.dataset.readingId ?? messages[indexOf(node) - earlier]?.id;

export function readingPoint(thread: HTMLElement, messages: Message[], earlier = 0): Reading | undefined {
  const box = thread.getBoundingClientRect();
  const top = box.top;
  const available = nodes(thread);
  const column = (thread.querySelector<HTMLElement>(':scope > .col') ?? thread).getBoundingClientRect();
  const x = (column.left + column.right) / 2;
  const start = Math.max(0, top), end = Math.min(innerHeight, box.bottom);
  let node: HTMLElement | undefined;
  // Hit testing starts in the viewport instead of laying out every earlier offscreen reply.
  if (x >= 0 && x < innerWidth && end > start) {
    for (const fraction of [0, 0.25, 0.5, 0.75]) {
      const y = Math.min(end - 1, start + 1 + (end - start) * fraction);
      const hit = document.elementsFromPoint(x, y).map((el) => el.closest<HTMLElement>(ANCHORS))
        .find((el) => el && el !== thread && thread.contains(el));
      if (!hit) continue;
      node = hit;
      for (let parent = node.parentElement?.closest<HTMLElement>(ANCHORS); parent && parent !== thread && thread.contains(parent); parent = parent.parentElement?.closest<HTMLElement>(ANCHORS)) node = parent;
      const at = available.indexOf(node);
      if (at < 0) { node = undefined; continue; }
      for (let i = at - 1; i >= 0; i--) {
        const previous = available[i]!;
        const rect = previous.getBoundingClientRect();
        if (!rect.width && !rect.height && !previous.getClientRects().length) continue;
        if (rect.bottom <= top) break;
        node = previous;
      }
      break;
    }
  }
  node ??= available.find((node) => node.getClientRects().length && node.getBoundingClientRect().bottom > top);
  const id = node && idOf(node, messages, earlier);
  return node && id ? { id, index: indexOf(node), offset: node.getBoundingClientRect().top - top,
    ...(node.dataset.readingRun ? { run: true } : {}) } : undefined;
}

export function readingWindow(messages: Message[], point: Reading, page: number, earlier = 0) {
  if (!messages.length) return undefined;
  const found = messages.findIndex((message) => message.id === point.id);
  const index = found < 0 ? Math.max(0, Math.min(point.index - earlier, messages.length - 1)) : found;
  const run = [...runsOf(messages)].find(([, run]) => run.replies.includes(index));
  const end = Math.min(messages.length, Math.max(index + page, (run?.[0] ?? index) + 1));
  return { point: { ...point, id: messages[index]!.id, index: index + earlier }, from: Math.max(0, index - Math.floor(page / 2)),
    until: end < messages.length ? end : undefined, reveal: point.run ? undefined : index };
}

export function restoreReading(thread: HTMLElement, point: Reading, messages: Message[], earlier = 0): void {
  const available = nodes(thread);
  // Check identity before geometry so restoring one anchor never lays out every offscreen reply.
  const visible = (node: HTMLElement) => node.getClientRects().length > 0;
  const node = available.find((node) => idOf(node, messages, earlier) === point.id && !!node.dataset.readingRun === !!point.run && visible(node))
    ?? available.find((node) => indexOf(node) >= point.index && visible(node)) ?? available.findLast(visible);
  if (!node) return;
  const box = node.getBoundingClientRect();
  // A shortened message still leaves its last line visible.
  const offset = Math.max(point.offset, -Math.max(0, box.height - 1));
  const delta = box.top - thread.getBoundingClientRect().top - offset;
  if (delta) thread.scrollTop += delta;
}

/** Avoid waiting for a compositor commit when the tail is already in view. */
export function followTail(thread: HTMLElement): void {
  let end = Math.max(0, thread.scrollHeight - thread.clientHeight);
  // The new-conversation home scrolls only until its composer is whole, never to the end: what sits under it is not news.
  const composer = thread.classList.contains('welcome') ? thread.querySelector('.composer') : null;
  if (composer) end = Math.min(end, Math.max(0, thread.scrollTop + composer.getBoundingClientRect().bottom - thread.getBoundingClientRect().bottom));
  if (Math.abs(thread.scrollTop - end) > 1) thread.scrollTop = end;
}