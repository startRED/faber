import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api } from '../../ui/api.ts';
import { watchSystem } from '../../ui/appearance.ts';
import { useClock } from '../../ui/clock.ts';
import { animateList, millis } from '../../ui/motion.ts';
import { Projects } from '../../ui/Projects.tsx';
import { Running } from '../../ui/Running.tsx';
import { check, equal, ok } from './check.ts';

// Lighter code (docs/design/looks.md, "One grammar"): no timer runs while nobody looks, animated lists share what they can, and
// the mission screens that nothing opened are gone from the page.

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));

type Interval = { id: number; ms: number; run: () => void };

/** Records the intervals the page starts and ends, so a check sees what is alive. */
function watchIntervals() {
  const alive = new Map<number, Interval>();
  const real = { set: window.setInterval, clear: window.clearInterval };
  window.setInterval = ((run: () => void, ms?: number) => {
    const id = (real.set as (handler: () => void, ms?: number) => number).call(window, run, ms);
    alive.set(id, { id, ms: ms ?? 0, run });
    return id;
  }) as typeof window.setInterval;
  window.clearInterval = ((id?: number) => {
    if (id !== undefined) alive.delete(id);
    real.clear.call(window, id);
  }) as typeof window.clearInterval;
  return {
    withMs: (ms: number) => [...alive.values()].filter((i) => i.ms === ms),
    stop: () => {
      for (const id of alive.keys()) real.clear.call(window, id);
      window.setInterval = real.set;
      window.clearInterval = real.clear;
    },
  };
}

const hide = (value: boolean) => {
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => value });
  document.dispatchEvent(new Event('visibilitychange'));
};

check('timers: the one-minute clock and the running column\'s 15 s tick have no timer while the window is hidden', async () => {
  const stopSystem = watchSystem();
  const intervals = watchIntervals();
  const original = api.setClock;
  let pushed = 0;
  api.setClock = async () => void pushed++;
  const root = document.createElement('div');
  document.body.append(root);
  try {
    // Screens other checks left mounted answer to the same window: count what they do alone first.
    hide(true);
    await settle();
    hide(false);
    await settle();
    const others = { clock: intervals.withMs(60_000).length, running: intervals.withMs(15_000).length };
    hide(true);
    await settle();
    pushed = 0;
    const dispose = render(() => {
      useClock();
      return createComponent(Running, { items: [], projects: [], onOpen: () => {}, onClose: () => {}, modal: false, ref: () => {} });
    }, root);
    await settle();
    equal(intervals.withMs(60_000).length, 0, 'hidden: no clock timer');
    equal(intervals.withMs(15_000).length, 0, 'hidden: no timer in the running column');
    equal(pushed, 0, 'hidden: nothing sent to the engine');
    hide(false);
    await settle();
    equal(intervals.withMs(60_000).length - others.clock, 1, 'shown: one clock timer');
    equal(intervals.withMs(15_000).length - others.running, 1, 'shown: one timer in the running column');
    ok(pushed >= 1, 'coming back sends the time zone once at once, as the minute may have passed');
    hide(true);
    await settle();
    equal(intervals.withMs(60_000).length + intervals.withMs(15_000).length, 0, 'hidden again: both timers end');
    dispose();
  } finally {
    api.setClock = original;
    intervals.stop();
    Reflect.deleteProperty(document, 'hidden');
    document.documentElement.classList.remove('hidden-win');
    stopSystem();
    root.remove();
  }
});

check('motion: animated lists share one window resize listener and ignore transitions that move nothing', async () => {
  const added: string[] = [];
  const removed: string[] = [];
  const add = window.addEventListener.bind(window);
  const remove = window.removeEventListener.bind(window);
  window.addEventListener = ((type: string, ...rest: unknown[]) => (added.push(type), (add as (...a: unknown[]) => void)(type, ...rest))) as typeof window.addEventListener;
  window.removeEventListener = ((type: string, ...rest: unknown[]) => (removed.push(type), (remove as (...a: unknown[]) => void)(type, ...rest))) as typeof window.removeEventListener;
  const hosts = [1, 2, 3, 4].map(() => document.body.appendChild(document.createElement('div')));
  const disposes: (() => void)[] = [];
  const reads = { n: 0 };
  const rect = HTMLElement.prototype.getBoundingClientRect;
  try {
    for (const host of hosts) disposes.push(render(() => { const el = document.createElement('div'); el.innerHTML = '<p>a</p><p>b</p>'; animateList(el); return el; }, host));
    await settle();
    ok(added.filter((t) => t === 'resize').length <= 1, 'four lists, at most one resize listener (none when another list already holds it)');
    // A hover fade deep inside a list moves nothing: it must not make the lists measure their children.
    const target = hosts[0]!.querySelector('p')!;
    HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) { reads.n++; return rect.call(this); };
    target.dispatchEvent(new TransitionEvent('transitionend', { bubbles: true, propertyName: 'background-color' }));
    equal(reads.n, 0, 'a colour fade measures nothing');
    target.dispatchEvent(new TransitionEvent('transitionend', { bubbles: true, propertyName: 'height' }));
    ok(reads.n > 0, 'a transition that moves things is measured again');
  } finally {
    HTMLElement.prototype.getBoundingClientRect = rect;
    for (const dispose of disposes) dispose();
    window.addEventListener = add as typeof window.addEventListener;
    window.removeEventListener = remove as typeof window.removeEventListener;
    for (const host of hosts) host.remove();
  }
  ok(removed.filter((t) => t === 'resize').length <= 1, 'the listener goes only with the last list');
});

check('mission screens: the Projects screen is one "Open a project" screen with no board, no start card and no poll', async () => {
  const intervals = watchIntervals();
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:1000px;height:700px';
  document.body.append(root);
  try {
    const dispose = render(() => createComponent(Projects, { vendors: ['claude'], projects: [], recent: [], running: [], onOpen: () => {}, onAdd: () => {}, onOpened: () => {}, onError: () => {} }), root);
    await settle();
    ok(root.querySelector('.welcome-grid'), 'it opens as the new conversation does');
    for (const gone of ['.start-card', '.mission-board', '.task-card', '#mission-goal', '.m-goal']) equal(root.querySelector(gone), null, `no ${gone}`);
    equal(intervals.withMs(30_000).length, 0, 'the 30 s poll of the mission board is gone');
    dispose();
  } finally {
    intervals.stop();
    root.remove();
  }
  const selectors = new Set<string>();
  for (const sheet of document.styleSheets) for (const rule of sheet.cssRules) if (rule instanceof CSSStyleRule) selectors.add(rule.selectorText);
  const left = ['.start-card', '.mission-board', '.task-card', '.task-col', '.ny-card', '.cost-card', '.m-goal', '.build-slots'].filter((s) => [...selectors].some((sel) => sel.includes(s)));
  equal(left.join(', '), '', 'the stylesheet holds no style of the removed screens');
});

check('motion: a timing token reads in milliseconds whether the browser hands it back as 180ms or as .18s', () => {
  const el = document.createElement('div');
  document.body.append(el);
  try {
    equal(millis(el, '--dur-test', 7), 7, 'a token that is not set gives the fallback');
    for (const [written, expected] of [['180ms', 180], ['.18s', 180], ['0.22s', 220], ['220ms', 220], ['2s', 2000]] as const) {
      el.style.setProperty('--dur-test', written);
      equal(millis(el, '--dur-test', 7), expected, written);
    }
  } finally {
    el.remove();
  }
});
