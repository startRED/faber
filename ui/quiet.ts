import { batch, createSignal } from 'solid-js';
import type { AppEvent } from './api.ts';

// Quiet during games (docs/design/looks.md, "Quiet during games"; R191). While a game or another full-screen app is in front of
// the shown window, the screen acts as if its window were hidden: no picture or animation moves, no row repaints on a
// timer, and the engine's events wait, folded into one, until the person comes back. Nothing opens or takes the focus.

/** How often the shell is asked while the window is shown without the focus. */
export const GAME_MS = 5000;

const [quiet, setSignal] = createSignal(false);
const listeners = new Set<(quiet: boolean) => void>();

/** True while a game is in front of the shown window. */
export { quiet };

/** Calls `listener` each time the screen goes quiet or comes back. Returns the function that stops it. */
export function onQuiet(listener: (quiet: boolean) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Goes quiet or comes back. CSS animations pause with `html.quiet-win` (styles.css), timers end with the signal. */
export function setQuiet(on: boolean): void {
  if (on === quiet()) return;
  document.documentElement.classList.toggle('quiet-win', on);
  batch(() => {
    setSignal(on);
    for (const listener of listeners) listener(on);
  });
}

/**
 * Asks `ask` at once and every `every` ms while `away` holds (the window shown, another app with the focus), and hands each
 * new answer to `apply`; the focus coming back or the window hiding is an answer of no at once. An answer that lands after
 * the person came back is dropped. Returns the function that stops watching.
 */
export function watchGames(
  ask: () => Promise<boolean>,
  apply: (quiet: boolean) => void = setQuiet,
  every = GAME_MS,
  away: () => boolean = () => !document.hidden && !document.hasFocus(),
): () => void {
  let timer: ReturnType<typeof setInterval> | undefined;
  let asked = 0;
  let last = false;
  const answer = (game: boolean) => {
    if (game === last) return;
    last = game;
    apply(game);
  };
  const look = () => {
    const mine = ++asked;
    ask().then(
      (game) => mine === asked && away() && answer(game),
      () => {},
    );
  };
  const follow = () => {
    clearInterval(timer);
    timer = undefined;
    asked++;
    if (!away()) return answer(false);
    look();
    timer = setInterval(look, every);
  };
  follow();
  window.addEventListener('focus', follow);
  window.addEventListener('blur', follow);
  document.addEventListener('visibilitychange', follow);
  return () => {
    clearInterval(timer);
    asked++;
    window.removeEventListener('focus', follow);
    window.removeEventListener('blur', follow);
    document.removeEventListener('visibilitychange', follow);
  };
}

/** What a held event replaces: a newer copy of the same message, the same queue or the same quota window. Events without
 * a key are all kept. */
export function heldKey(e: AppEvent): string | undefined {
  switch (e.type) {
    case 'message':
      return `message:${e.conversationId}:${e.message.id}`;
    case 'queue':
      return `queue:${e.conversationId}`;
    case 'context':
      return `context:${e.conversationId}`;
    case 'quota':
      return `quota:${e.vendor}:${e.window}`;
    case 'running':
    case 'usage':
    case 'list':
      return e.type;
    default:
      return undefined;
  }
}

/** The running list with the higher revision wins, as in the store; every other event, the one that came last. */
const keep = (held: AppEvent, next: AppEvent): AppEvent =>
  held.type === 'running' && next.type === 'running' && held.revision > next.revision ? held : next;

/**
 * Hands events to `deliver` as they come, except while held: then they wait, one per key (`heldKey`) in the place the first
 * of its kind came, and go in one batch when the hold ends, so the screen repaints once for the whole time away.
 */
export function createHold(deliver: (event: AppEvent) => void) {
  let held: Map<string | number, AppEvent> | undefined;
  let unkeyed = 0;
  return {
    push(event: AppEvent) {
      if (!held) return deliver(event);
      const key = heldKey(event) ?? unkeyed++;
      const before = held.get(key);
      held.set(key, before ? keep(before, event) : event);
    },
    hold(on: boolean) {
      if (on) {
        held ??= new Map();
        return;
      }
      const waiting = held;
      held = undefined;
      if (waiting) batch(() => { for (const event of waiting.values()) deliver(event); });
    },
    /** How many events wait. */
    get waiting() {
      return held?.size ?? 0;
    },
  };
}
