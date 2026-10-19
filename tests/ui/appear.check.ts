import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import type { Project, UpdateStatus } from '../../ui/api.ts';
import { Collapse, Fold, Opt, PickMenu } from '../../ui/controls.tsx';
import { UpdateLine } from '../../ui/UpdateStage.tsx';
import { millis } from '../../ui/motion.ts';
import { Welcome } from '../../ui/Welcome.tsx';
import { check, equal, ok } from './check.ts';

// Nothing on a screen appears, leaves or moves in one frame (the owner, 2026-10-17): what opens, closes, swaps or arrives
// eases, and under reduced or no motion it is instant. Each check below fails when its part goes back to jumping.

const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const exit = () => millis(document.documentElement, '--dur-exit', 180);

/** A page that is in full motion whatever the PC asks for: the system preference is read through one media query. The mock
 * reaches the script only. A check that needs the stylesheet's own transitions (`css`) stands down when the PC asks for less
 * motion, like the other motion checks: the stylesheet cancels every transition then, and CI's machines ask for that. */
async function inFullMotion(body: () => Promise<void>, css = true): Promise<void> {
  if (css && matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const preference = Object.getOwnPropertyDescriptor(MediaQueryList.prototype, 'matches')!;
  Object.defineProperty(MediaQueryList.prototype, 'matches', { ...preference, get(this: MediaQueryList) {
    return this.media === '(prefers-reduced-motion: reduce)' ? false : preference.get!.call(this);
  } });
  try { await body(); } finally { Object.defineProperty(MediaQueryList.prototype, 'matches', preference); }
}

/** Draws a component where the stylesheet and the theme's timing apply. */
function drawn(view: () => ReturnType<typeof createComponent>, motion = 'full') {
  const host = document.createElement('div');
  host.dataset.motion = motion;
  host.style.cssText = 'position:fixed;left:10px;top:10px;width:700px';
  for (const [key, value] of Object.entries(themes.css.base)) host.style.setProperty(`--${key}`, value);
  document.body.append(host);
  const stop = render(view, host);
  return { host, stop: () => { stop(); host.remove(); } };
}

check('appear: the "?" text of a row grows open and shrinks away, and is out of the page only once it has shrunk', async () => inFullMotion(async () => {
  const view = drawn(() => createComponent(Opt, { id: 'x', label: 'Name', more: 'The long text.' }));
  try {
    const q = view.host.querySelector<HTMLButtonElement>('.st-q')!;
    equal(view.host.querySelector('.collapse'), null, 'closed: nothing in the page');
    q.click();
    await settle(0);
    const wrap = view.host.querySelector<HTMLElement>('.collapse')!;
    ok(wrap?.classList.contains('open') && wrap.querySelector('.st-more'), 'open: the text is there');
    ok(wrap.getAnimations().length > 0, 'it grows from nothing instead of appearing at its height');
    equal(getComputedStyle(wrap.firstElementChild!).overflow, 'hidden', 'clipped while it grows');
    await settle(exit() + 120);
    equal(wrap.getAnimations().length, 0, 'and rests');
    equal(getComputedStyle(wrap.firstElementChild!).overflow, 'visible', 'free once it rests, so a focus ring is never cut');
    q.click();
    await settle(0);
    ok(view.host.querySelector('.collapse'), 'closing keeps it in the page while it shrinks');
    ok(!view.host.querySelector('.collapse.open'), 'closing: no longer open');
    ok(view.host.querySelector('.collapse')!.getAnimations().length > 0, 'it shrinks instead of vanishing');
    await settle(exit() + 120);
    equal(view.host.querySelector('.collapse'), null, 'closed again: out of the page');
  } finally { view.stop(); }
}));

check('appear: a folded group opens and closes the same way, and the stylesheet cancels the transition at the reduced level', async () => inFullMotion(async () => {
  const view = drawn(() => createComponent(Fold, { id: 'f', title: 'More', children: 'inside' }));
  try {
    view.host.querySelector<HTMLButtonElement>('.st-fold-head')!.click();
    await settle(0);
    ok(view.host.querySelector('.st-fold .collapse.open')!.getAnimations().length > 0, 'the group grows open');
    view.host.querySelector<HTMLButtonElement>('.st-fold-head')!.click();
    await settle(0);
    ok(view.host.querySelector('.st-fold-body'), 'and stays while it shrinks');
    await settle(exit() + 120);
    equal(view.host.querySelector('.st-fold-body'), null);
  } finally { view.stop(); }
  // The transition itself is cancelled at the reduced level by the stylesheet.
  const still = drawn(() => createComponent(Collapse, { open: true, children: 'x' }), 'reduced');
  try {
    equal(getComputedStyle(still.host.querySelector('.collapse')!).transitionDuration, '0s');
    equal(still.host.querySelector('.collapse')!.getAnimations().length, 0);
  } finally { still.stop(); }
}));

check('appear: the permission menu opens from its corner and is still there, fading, for its short exit', async () => inFullMotion(async () => {
  const picks: string[] = [];
  const view = drawn(() => createComponent(PickMenu, { id: 'p', label: 'Pick', value: 'a', options: [['a', 'A', 'first'], ['b', 'B', 'second']] as [string, string, string][], onPick: (v: string) => void picks.push(v) }));
  try {
    const button = view.host.querySelector<HTMLButtonElement>('#p')!;
    button.click();
    await settle(0);
    const menu = view.host.querySelector<HTMLElement>('.st-menu')!;
    ok(menu && !menu.classList.contains('leaving'));
    ok(menu.getAnimations().length > 0, 'it eases in');
    button.click();
    await settle(0);
    const going = view.host.querySelector<HTMLElement>('.st-menu.leaving')!;
    ok(going, 'closing keeps the menu in the page while it fades');
    ok(going.inert, 'and nothing in it can be reached meanwhile');
    equal(button.getAttribute('aria-expanded'), 'false');
    await settle(exit() + 120);
    equal(view.host.querySelector('.st-menu'), null, 'then it is gone');
  } finally { view.stop(); }
}));

check('appear: the update line grows in and shrinks away with its last words, so the rows under it never jump', async () => inFullMotion(async () => {
  const [status, set] = createSignal<UpdateStatus>({ phase: 'none' });
  const view = drawn(() => createComponent(UpdateLine, { get status() { return status(); } }));
  try {
    equal(view.host.querySelector('.upd'), null);
    set({ phase: 'downloading', percent: 42 });
    await settle(0);
    ok(view.host.querySelector('.collapse.open .upd'), 'the line is there');
    ok(view.host.querySelector('.collapse')!.getAnimations().length > 0, 'it grew in');
    set({ phase: 'ready' });
    await settle(0);
    const line = view.host.querySelector('.upd')!;
    ok(line && line.textContent!.includes('42'), 'it leaves with the words it had');
    await settle(exit() + 120);
    equal(view.host.querySelector('.upd'), null);
  } finally { view.stop(); }
}));

check('appear: the home between a project and a chat swaps its header with a cross-fade', async () => inFullMotion(async () => {
  const project: Project = { id: 'p1', name: 'Atlas', path: 'C:/work/atlas' } as Project;
  const [open, setOpen] = createSignal<Project | undefined>();
  const view = drawn(() => createComponent(Welcome, {
    recent: [], running: [], projects: [],
    get project() { return open(); },
    onSuggest: () => {}, onOpen: () => {}, children: 'composer',
  }));
  try {
    await settle(50);
    equal(view.host.querySelector('.p-header'), null, 'the chat home has no project header');
    const before = view.host.querySelector('.hello')!.textContent;
    setOpen(project);
    await settle(30);
    const header = view.host.querySelector<HTMLElement>('.p-header');
    ok(header, 'the project header is there');
    ok(header.getAnimations().length > 0, 'it arrives instead of appearing');
    ok(view.host.querySelector('.hello')!.textContent !== before, 'the greeting is the project one');
    const copy = document.querySelector('.motion-exit')?.shadowRoot?.querySelector('.hello');
    ok(copy, 'the old greeting leaves as an inert copy');
    await settle(400);
    equal(document.querySelector('.motion-exit'), null, 'and the copy is removed when it has faded');
    setOpen(undefined);
    await settle(30);
    ok(document.querySelector('.motion-exit')?.shadowRoot?.querySelector('.p-header'), 'the project header leaves as a copy too');
    await settle(400);
  } finally { view.stop(); }
}, false));
