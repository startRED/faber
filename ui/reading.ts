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

const nodes = (thread: HTMLElement) => [...thread.querySelectorAll<HTMLElement>('[data-message-index], [data-reading-id]')];
const indexOf = (node: HTMLElement) => Number(node.dataset.readingIndex ?? node.dataset.messageIndex);
const idOf = (node: HTMLElement, messages: Message[], earlier: number) => node.dataset.readingId ?? messages[indexOf(node) - earlier]?.id;

export function readingPoint(thread: HTMLElement, messages: Message[], earlier = 0): Reading | undefined {
  const top = thread.getBoundingClientRect().top;
  const node = nodes(thread).find((node) => node.getClientRects().length && node.getBoundingClientRect().bottom > top);
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
  const available = nodes(thread).filter((node) => node.getClientRects().length);
  const node = available.find((node) => idOf(node, messages, earlier) === point.id && !!node.dataset.readingRun === !!point.run)
    ?? available.find((node) => indexOf(node) >= point.index) ?? available.at(-1);
  if (!node) return;
  const box = node.getBoundingClientRect();
  // A shortened message still leaves its last line visible.
  const offset = Math.max(point.offset, -Math.max(0, box.height - 1));
  thread.scrollTop += box.top - thread.getBoundingClientRect().top - offset;
}
