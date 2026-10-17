import { createComponent, createEffect, createRoot, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { AppEvent, Message } from '../../ui/api.ts';
import { watchSystem, windowHidden } from '../../ui/appearance.ts';
import { idleLife } from '../../ui/idle.ts';
import { createHold, quiet, setQuiet, watchGames } from '../../ui/quiet.ts';
import { Running } from '../../ui/Running.tsx';
import { check, deepEqual, equal, ok } from './check.ts';
import { watchIntervals } from './lighter.check.ts';

// Quiet during games (docs/design/looks.md, "Quiet during games"): with a game in front of the shown window, nothing moves, no
// row repaints on a timer and the engine's events wait as one; the person's return brings them all at once.

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));

check('quiet: a game in front stops animations, idle life and the running column\'s tick, and coming back starts them', async () => {
  // The test build's window is hidden: show it, so only a game can make the screen rest.
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
  const stopSystem = watchSystem();
  const intervals = watchIntervals();
  const root = document.createElement('div');
  const moving = document.createElement('div');
  moving.style.animation = 'blink 1s infinite';
  document.body.append(root, moving);
  try {
    // Screens other checks left mounted answer to the same signal: count what they do alone first.
    const others = intervals.withMs(15_000).length;
    const dispose = render(() => createComponent(Running, { items: [], projects: [], onOpen: () => {}, onClose: () => {}, modal: false, ref: () => {} }), root);
    await settle();
    equal(intervals.withMs(15_000).length - others, 1, 'no game: the running column ticks');
    equal(getComputedStyle(moving).animationPlayState, 'running', 'no game: animations run');
    setQuiet(true);
    await settle();
    ok(windowHidden(), 'a game in front counts as a hidden window');
    equal(idleLife(), false, 'no idle life behind a game');
    equal(intervals.withMs(15_000).length, 0, 'a game in front: no timer in the running column');
    equal(getComputedStyle(moving).animationPlayState, 'paused', 'a game in front: every animation pauses');
    setQuiet(false);
    await settle();
    equal(intervals.withMs(15_000).length - others, 1, 'back: the tick runs again');
    equal(getComputedStyle(moving).animationPlayState, 'running', 'back: animations run again');
    dispose();
  } finally {
    setQuiet(false);
    intervals.stop();
    stopSystem();
    Reflect.deleteProperty(document, 'hidden');
    document.dispatchEvent(new Event('visibilitychange'));
    root.remove();
    moving.remove();
  }
});

check('quiet: the shell is asked only while the window is shown without the focus, and a late answer is dropped', async () => {
  let away = true;
  let asked = 0;
  let answer: (game: boolean) => void = () => {};
  const applied: boolean[] = [];
  const stop = watchGames(
    () => {
      asked++;
      return new Promise<boolean>((resolve) => (answer = resolve));
    },
    (game) => applied.push(game),
    20,
    () => away,
  );
  try {
    equal(asked, 1, 'away at the start: asked at once');
    answer(true);
    await settle(5);
    deepEqual(applied, [true], 'a game in front goes quiet');
    await settle(50);
    ok(asked >= 2, 'asked again while away');
    // The person comes back while an answer is on its way: no at once, and the answer that lands later says nothing.
    away = false;
    window.dispatchEvent(new Event('focus'));
    answer(true);
    await settle(50);
    deepEqual(applied, [true, false], 'the focus back ends quiet at once and a late yes is dropped');
    const before = asked;
    await settle(60);
    equal(asked, before, 'with the focus, the shell is not asked');
  } finally {
    stop();
  }
});

const message = (id: string, text: string): Message => ({ id, role: 'assistant', text, at: 0 }) as Message;

check('quiet: events wait folded into one per message, queue and list, and land in one batch on return', () => {
  const delivered: AppEvent[] = [];
  const hold = createHold((e) => delivered.push(e));
  hold.push({ type: 'list' });
  equal(delivered.length, 1, 'no game: events pass at once');
  delivered.length = 0;
  hold.hold(true);
  hold.push({ type: 'message', conversationId: 'c', message: message('m1', 'a') });
  hold.push({ type: 'running', revision: 7, items: [] });
  hold.push({ type: 'message', conversationId: 'c', message: message('m2', 'x') });
  hold.push({ type: 'message', conversationId: 'c', message: message('m1', 'ab') });
  hold.push({ type: 'running', revision: 5, items: [] });
  hold.push({ type: 'list' });
  hold.push({ type: 'list' });
  equal(delivered.length, 0, 'nothing lands while a game is in front');
  equal(hold.waiting, 4, 'one per message, one running list, one list');
  // Every delivery sets a signal; one batch means the effect that reads it runs once for all of them.
  const [seen, setSeen] = createSignal(0);
  let runs = 0;
  const disposeRoot = createRoot((dispose) => {
    createEffect(() => { seen(); runs++; });
    return dispose;
  });
  const counting = createHold((e) => { delivered.push(e); setSeen((n) => n + 1); });
  counting.hold(true);
  for (const e of [{ type: 'list' }, { type: 'usage', vendors: [] }] as AppEvent[]) counting.push(e);
  runs = 0;
  counting.hold(false);
  equal(runs, 1, 'the screen repaints once for the whole time away');
  disposeRoot();
  delivered.length = 0;
  hold.hold(false);
  deepEqual(
    delivered.map((e) => (e.type === 'message' ? `${e.message.id}:${e.message.text}` : e.type === 'running' ? `running:${e.revision}` : e.type)),
    ['m1:ab', 'running:7', 'm2:x', 'list'],
    'the newest copy of each, in the order each first came; the higher revision wins',
  );
  hold.push({ type: 'list' });
  equal(delivered.length, 5, 'back: events pass at once again');
  equal(quiet(), false);
});
