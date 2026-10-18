import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import type { Project, Summary } from '../../ui/api.ts';
import { Palette } from '../../ui/Palette.tsx';
import { rank } from '../../ui/palette.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
/** A key press on the window; false means the page stopped the browser's own action. */
const press = (init: KeyboardEventInit, to: EventTarget = window) => to.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }));
const ctrlK = () => press({ key: 'k', ctrlKey: true });

const names = ['Plan the trip', 'Café menu', 'Fix the login', 'Tax papers', 'Garden notes', 'Old idea'];
const conversations: Summary[] = names.map((title, i) => ({
  id: `c${i}`, title, area: i === 2 ? 'project' : 'chat', projectId: i === 2 ? 'p1' : undefined, updatedAt: 1_000 - i,
}));
const projects: Project[] = [{ id: 'p1', name: 'Garden app', path: 'X:/garden' }];

function stage(motion = 'none') {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:600px';
  root.dataset.motion = motion;
  for (const [name, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${name}`, value);
  document.body.append(root);
  return root;
}

function mount(root: HTMLElement, calls: string[], enabled: () => boolean = () => true) {
  return render(
    () =>
      createComponent(Palette, {
        get enabled() { return enabled(); },
        conversations, projects,
        onOpen: (c) => calls.push(`open ${c.id}`),
        onProject: (id) => calls.push(`project ${id}`),
        onNew: () => calls.push('new'),
        onArea: (area) => calls.push(`area ${area}`),
        onSettings: (section) => calls.push(`settings ${section ?? ''}`),
        onRunning: () => calls.push('running'),
        onAddProject: () => calls.push('add'),
      }),
    root,
  );
}

const box = () => document.querySelector<HTMLInputElement>('.pal input');
const options = () => [...document.querySelectorAll<HTMLElement>('.pal [role=option]')].map((o) => o.querySelector('.pal-t')!.textContent);
const type = (text: string) => {
  const input = box()!;
  input.value = text;
  input.dispatchEvent(new InputEvent('input', { bubbles: true }));
};

check('palette: a few letters match loosely, ignoring case and accents, with every word needed and the name before the hint', () => {
  const list = [
    { label: 'New conversation' },
    { label: 'Settings: Appearance', hint: undefined },
    { label: 'Café menu', hint: 'Chat' },
    { label: 'Fix the login', hint: 'Garden app' },
    { label: 'Garden notes', hint: 'Chat' },
  ];
  deepEqual(rank(list, '').length, 0, 'an empty search ranks nothing');
  deepEqual(rank(list, 'CAFE').map((e) => e.label), ['Café menu'], 'case and accents do not matter');
  deepEqual(rank(list, 'stg').map((e) => e.label), ['Settings: Appearance'], 'letters in order, with others between');
  deepEqual(rank(list, 'new conv').map((e) => e.label), ['New conversation'], 'every word must match');
  deepEqual(rank(list, 'new zzz').length, 0, 'a word that matches nothing rules the entry out');
  deepEqual(rank(list, 'garden').map((e) => e.label), ['Garden notes', 'Fix the login'], 'the name outranks the hint, and the hint still finds');
  equal(rank(list, 'e', 2).length, 2, 'the list is cut at the limit');
});

check('palette: Ctrl+K opens one box (recent conversations, then actions), typing finds across areas, Enter picks, Esc gives focus back', async () => {
  const root = stage();
  const calls: string[] = [];
  const opener = document.createElement('button');
  opener.textContent = 'before';
  root.append(opener);
  const dispose = mount(root, calls);
  try {
    opener.focus();
    equal(document.querySelector('.pal-scrim'), null, 'nothing of it exists while it is closed');
    equal(ctrlK(), false, 'Ctrl K is Faber\'s');
    await settle();
    equal(document.querySelectorAll('.pal-scrim').length, 1);
    equal(document.activeElement, box(), 'the box takes focus');
    equal(document.querySelector('.pal')?.getAttribute('role'), 'dialog');
    const empty = options();
    deepEqual(empty.slice(0, 5), names.slice(0, 5), 'the latest five conversations come first, newest first');
    ok(empty.includes(t('side.new')) && empty.includes(t('run.toggle')), 'then the actions');
    deepEqual([...document.querySelectorAll('.pal-head')].map((h) => h.textContent), [t('palette.recent'), t('palette.actions')]);
    // Typing ranks over both areas, the projects and the actions.
    type('garden');
    deepEqual(options(), ['Garden app', 'Garden notes', 'Fix the login']);
    type('appear');
    deepEqual(options(), [t('palette.settings', { section: t('set.appearance') })]);
    type('zzzz');
    equal(options().length, 0);
    ok(document.querySelector('.pal-none'), 'a search that finds nothing says so');
    type('fix lo');
    equal(document.querySelector('.pal [aria-selected=true]')?.textContent?.startsWith('Fix the login'), true);
    equal(press({ key: 'ArrowDown' }, box()!), false);
    equal(box()!.getAttribute('aria-activedescendant'), 'pal-0', 'one result keeps the choice on it');
    type('c');
    const before = box()!.getAttribute('aria-activedescendant');
    press({ key: 'ArrowDown' }, box()!);
    ok(box()!.getAttribute('aria-activedescendant') !== before, 'the arrows move the choice');
    type('fix lo');
    press({ key: 'Enter' }, box()!);
    deepEqual(calls, ['open c2']);
    await settle(300);
    equal(document.querySelector('.pal-scrim'), null, 'picking closes it and takes it out of the page');
    equal(document.activeElement, opener, 'focus goes back to where it was');
    // An action runs the same way, and Esc closes without running anything.
    ctrlK();
    await settle();
    type('go to proj');
    press({ key: 'Enter' }, box()!);
    equal(calls[1], 'area project', 'an action runs what the sidebar button runs');
    await settle(300);
    ctrlK();
    await settle();
    let heard = 0;
    const count = (e: KeyboardEvent) => e.key === 'Escape' && heard++;
    window.addEventListener('keydown', count);
    press({ key: 'Escape' }, box()!);
    window.removeEventListener('keydown', count);
    equal(heard, 0, 'Esc stops here: it does not also close what is under the box');
    await settle(300);
    equal(document.querySelector('.pal-scrim'), null);
    equal(document.activeElement, opener);
    equal(calls.length, 2);
  } finally {
    dispose();
    root.remove();
  }
});

check('palette: Ctrl+K again closes it, a click outside closes it, and Ctrl N, Ctrl F wait under it', async () => {
  const root = stage();
  const dispose = mount(root, []);
  try {
    ctrlK();
    await settle();
    ok(box());
    equal(press({ key: 'n', ctrlKey: true }), false, 'the shortcut underneath is stopped');
    ctrlK();
    await settle(300);
    equal(document.querySelector('.pal-scrim'), null, 'Ctrl K toggles');
    ctrlK();
    await settle();
    document.querySelector('.pal-scrim')!.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    await settle(300);
    equal(document.querySelector('.pal-scrim'), null, 'a click on the dimmed screen closes it');
    ctrlK();
    await settle();
    document.querySelector('.pal')!.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    await settle(300);
    ok(box(), 'a click inside the box keeps it');
  } finally {
    dispose();
    root.remove();
  }
});

check('palette: it does not open while the first run, the tutorial or an update covers the screen', async () => {
  const root = stage();
  const [on, setOn] = createSignal(false);
  const dispose = mount(root, [], on);
  try {
    ctrlK();
    await settle();
    equal(document.querySelector('.pal-scrim'), null);
    setOn(true);
    ctrlK();
    await settle();
    ok(box());
    setOn(false);
    await settle(300);
    equal(document.querySelector('.pal-scrim'), null, 'it closes when something takes over');
  } finally {
    dispose();
    root.remove();
  }
});

check('palette: it scales and fades in, leaves with a shorter exit, and does not move in reduced motion', async () => {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const root = stage('full');
  const dispose = mount(root, []);
  try {
    ctrlK();
    await settle(10);
    const dialog = document.querySelector<HTMLElement>('.pal')!;
    const entering = dialog.getAnimations().find((a) => (a as CSSAnimation).animationName === 'pal-in');
    ok(entering, 'the box has an entrance');
    const enter = Number(entering!.effect!.getComputedTiming().duration);
    entering!.pause();
    entering!.currentTime = 0;
    equal(getComputedStyle(dialog).opacity, '0', 'it starts invisible');
    ok(new DOMMatrix(getComputedStyle(dialog).transform).a < 1, 'and a little smaller');
    entering!.finish();
    await settle(10);
    ctrlK();
    await settle(10);
    const leaving = document.querySelector<HTMLElement>('.pal-scrim.leaving');
    ok(leaving, 'the box is still there while it leaves');
    const exit = leaving!.querySelector<HTMLElement>('.pal')!.getAnimations().find((a) => (a as CSSAnimation).animationName === 'pal-out');
    ok(exit && Number(exit.effect!.getComputedTiming().duration) < enter, 'the exit is shorter than the entrance');
    ok(leaving!.hasAttribute('inert'), 'a leaving box takes no input');
    await settle(320);
    equal(document.querySelector('.pal-scrim'), null);
    root.dataset.motion = 'reduced';
    ctrlK();
    await settle(10);
    equal(document.querySelector('.pal')!.getAnimations().length, 0, 'reduced motion shows it at once');
  } finally {
    dispose();
    root.remove();
  }
});

check('palette: Tab stays in the box without reaching the shell, and an open right-click menu owns Enter and the arrows', async () => {
  const root = stage();
  const calls: string[] = [];
  const dispose = mount(root, calls);
  try {
    ctrlK();
    await settle();
    let heard = 0;
    const count = (e: KeyboardEvent) => e.key === 'Tab' && heard++;
    window.addEventListener('keydown', count);
    equal(press({ key: 'Tab' }, box()!), false, 'Tab does not leave the box');
    window.removeEventListener('keydown', count);
    equal(heard, 0, 'and the shell\'s own focus trap never hears it');
    equal(document.activeElement, box());
    type('fix lo');
    const menu = document.createElement('div');
    menu.className = 'cmenu';
    document.body.append(menu);
    equal(press({ key: 'Enter' }, box()!), true, 'Enter belongs to the menu while it is open');
    menu.remove();
    deepEqual(calls, []);
    press({ key: 'Enter' }, box()!);
    deepEqual(calls, ['open c2'], 'and to the box again once it closes');
  } finally {
    dispose();
    root.remove();
  }
});
