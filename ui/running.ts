import { createSignal } from 'solid-js';
import { type Area, errorText, type RunningItem, type RunningList } from './api.ts';

// The running list on the screen (docs/design/layout.md, "Running column"). The engine builds and orders the list; the screen
// keeps the newest one it has seen, groups it into blocks and decides when the column opens by itself.

/** The newer of two lists. The same revision keeps the one already held. */
export function newest(held: RunningList, offered: RunningList): RunningList {
  return offered.revision > held.revision ? offered : held;
}

/** The list the screen shows. The snapshot from `state()` and every `Running` event are offered to it, in whatever order
 * they arrive; an old list never replaces a newer one. */
export function createRunningStore() {
  const [list, setList] = createSignal<RunningList>({ revision: -1, items: [] });
  return { list, offer: (next: RunningList) => setList((held) => newest(held, next)) };
}

export const store = createRunningStore();

export interface Block {
  /** The project id, or `chat` for the conversations of the Chat area. */
  key: string;
  items: RunningItem[];
  /** How many of its items wait for the user. */
  needs: number;
}

/** The engine's order, cut into one block per project plus one for Chat. A block keeps the place of its first item. */
export function blocksOf(items: RunningItem[]): Block[] {
  const blocks = new Map<string, Block>();
  for (const item of items) {
    const key = item.area === 'project' && item.projectId ? item.projectId : 'chat';
    const block = blocks.get(key) ?? { key, items: [], needs: 0 };
    block.items.push(item);
    if (item.state === 'needsYou') block.needs++;
    blocks.set(key, block);
  }
  return [...blocks.values()];
}

/** The conversations the new-conversation screen offers to pick up: those that wait for the user first, then the latest, at
 * most `limit`. `recent` is every conversation, newest first: it is ordered whole, so a waiting one is found however old it is. */
export function pickUp<T extends { id: string; archived?: boolean }>(all: T[], items: RunningItem[], limit = 4): T[] {
  // An archived conversation is put away: it is picked up only from search.
  const recent = all.filter((c) => !c.archived);
  const needs = (c: T) => items.some((i) => i.conversationId === c.id && i.state === 'needsYou');
  return [...recent.filter(needs), ...recent.filter((c) => !needs(c))].slice(0, limit);
}

/** A question is named by its conversation, its turn and its request together: a request id alone repeats (Codex counts
 * from 0 in every server). */
export const askKey = (item: Pick<RunningItem, 'conversationId' | 'turnId'>, requestId: string) => `${item.conversationId}:${item.turnId}:${requestId}`;

export const waitingKeys = (items: RunningItem[]) => items.flatMap((item) => item.permissions.map((p) => askKey(item, p.requestId)));

export const QUIET_MS = 1500;

/** When the column opens by itself: once for each question, and not while the user is typing. */
export function createAutoOpen(quiet = QUIET_MS) {
  const shown = new Set<string>();
  let typed = Number.NEGATIVE_INFINITY;
  return {
    /** The user typed in a text field at `now` (milliseconds). */
    typed(now: number) {
      typed = now;
    },
    /** These questions were shown to the user (the column was open, or the user closed it): none of them opens it again. */
    seen(keys: string[]) {
      for (const key of keys) shown.add(key);
    },
    /** `open`: show the column now; every question waiting then counts as shown, so none of them opens it again, even if
     * the user closes the column. Otherwise `retryIn` is how long until it may open (it is waiting for the typing to
     * stop), or undefined when no new question waits. */
    check(keys: string[], now: number): { open: boolean; retryIn?: number } {
      const fresh = keys.filter((key) => !shown.has(key));
      if (fresh.length === 0) return { open: false };
      const wait = typed + quiet - now;
      if (wait > 0) return { open: false, retryIn: wait };
      for (const key of fresh) shown.add(key);
      return { open: true };
    },
  };
}

/** Where the user is: the area, the project of a project conversation, and the conversation. */
export type Place = { area: Area; projectId?: string; id?: string };

/** The place of a conversation the user opens, from the place they are in. A project conversation sets the project; a chat
 * leaves the last project as it was, so the Projects area comes back to it. Area, project and id go in together. */
export function openPlace(from: Place, c: { id: string; area: Area; projectId?: string }): Place {
  return { area: c.area, projectId: c.area === 'project' ? c.projectId : from.projectId, id: c.id };
}

export const placeOfRunning = (from: Place, item: RunningItem): Place => openPlace(from, { id: item.conversationId, area: item.area, projectId: item.projectId });

/** What an answer that was refused means for its card: a stale question is dropped quietly; anything else is shown by its
 * error key (an undelivered answer keeps the card so the user can try again). */
export function answerOutcome(err: unknown): 'stale' | { problem: string } {
  const key = errorText(err);
  return key === 'error.permission.stale' ? 'stale' : { problem: key };
}

/** How long a turn has been running, as a translation key and a count. */
export function elapsed(since: number, now: number): { key: 'time.seconds' | 'time.minutes' | 'time.hours'; n: number } {
  const seconds = Math.max(0, Math.floor((now - since) / 1000));
  if (seconds < 60) return { key: 'time.seconds', n: seconds };
  return seconds < 3600 ? { key: 'time.minutes', n: Math.floor(seconds / 60) } : { key: 'time.hours', n: Math.floor(seconds / 3600) };
}

/** How long the prompt cache of a reply stays warm: the milliseconds left, or undefined when it is unknown or already cold. */
export function warmLeft(until: number | undefined, now: number): number | undefined {
  return until !== undefined && until > now ? until - now : undefined;
}
