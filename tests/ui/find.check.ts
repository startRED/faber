import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Find } from '../../ui/Find.tsx';
import { installContextMenu } from '../../ui/contextmenu.ts';
import { LIMIT, rangesIn } from '../../ui/find.ts';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const press = (init: KeyboardEventInit, to: EventTarget = window) => to.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }));
const ctrlF = () => press({ key: 'f', ctrlKey: true });

async function until(done: () => boolean, ms = 1500) {
  for (let waited = 0; !done() && waited < ms; waited += 25) await settle(25);
  ok(done(), 'it did not happen in time');
}

const FILLER = '<p style="height:90px;margin:0">Nothing to see here.</p>'.repeat(8);
const THREAD = `<div class="col">
  <div class="mine"><div class="you"><p>Café con leche</p></div></div>
  <div class="ai"><div class="md"><p>The cafe opens at nine. CAFE again.</p><button>cafe button</button><span class="sr">cafe sr</span><p style="display:none">cafe hidden</p></div></div>
  <div class="tools">cafe in the work log</div>
  <div class="ai"><div class="md">${FILLER}<p>The last cafe.</p></div></div>
</div>`;

function stage(motion = 'none') {
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:400px';
  host.dataset.motion = motion;
  for (const [name, value] of Object.entries(themes.css.base)) host.style.setProperty(`--${name}`, value);
  const thread = document.createElement('div');
  thread.className = 'thread';
  thread.style.cssText = 'position:absolute;inset:0;overflow-y:auto';
  thread.innerHTML = THREAD;
  host.append(thread);
  document.body.append(host);
  return { host, thread };
}

const bar = () => document.querySelector<HTMLElement>('.find');
const input = () => document.querySelector<HTMLInputElement>('.find input');
const count = () => document.querySelector('.find-count')?.textContent;
const type = (text: string) => {
  input()!.value = text;
  input()!.dispatchEvent(new InputEvent('input', { bubbles: true }));
};
const marks = (name: string) => (CSS.highlights.get(name) ? [...CSS.highlights.get(name)!].length : 0);

check('find: the matches are the readable text only, ignoring case and accents, and the text is never rewritten', () => {
  const { host, thread } = stage();
  try {
    const before = thread.innerHTML;
    const { ranges, capped } = rangesIn(thread, 'CAFE');
    equal(ranges.length, 4, 'Café, cafe, CAFE and the last cafe: not a button, a screen-reader line, a hidden line or the work log');
    equal(capped, false);
    equal(ranges[0]!.toString(), 'Café', 'the range holds the original letters');
    equal(rangesIn(thread, '   ').ranges.length, 0, 'a blank search finds nothing');
    equal(thread.innerHTML, before, 'no mark was added to a message');
    thread.querySelector('.md')!.insertAdjacentHTML('beforeend', `<p>${'a'.repeat(LIMIT + 5)}</p>`);
    const many = rangesIn(thread, 'a');
    equal(many.ranges.length, LIMIT);
    equal(many.capped, true, 'a letter typed in a long message stops at the limit and says so');
  } finally {
    host.remove();
  }
});

check('find: Ctrl+F opens the bar, counts and paints the matches, Enter and Shift+Enter go round, Esc closes it and the page is as it was', async () => {
  const { host, thread } = stage();
  // The page itself may run in full motion: the thread's own level (none here) decides, so the scroll below is instant.
  const page = document.documentElement.dataset.motion;
  document.documentElement.dataset.motion = 'full';
  const opener = document.createElement('button');
  host.append(opener);
  const dispose = render(() => createComponent(Find, { enabled: true, root: () => thread }), host);
  const before = thread.innerHTML;
  try {
    opener.focus();
    equal(bar(), null, 'nothing of it exists while it is closed');
    equal(ctrlF(), false, 'the browser\'s own find is stopped');
    await settle();
    ok(bar(), 'the bar is open');
    equal(document.activeElement, input(), 'the box has the focus');
    type('cafe');
    equal(count(), t('find.count', { current: 1, total: 4 }), 'the count reads "1 of 4"');
    equal(marks('find'), 4, 'every match is painted');
    equal(marks('find-now'), 1, 'the current one is painted apart');
    press({ key: 'Enter' }, input()!);
    equal(count(), t('find.count', { current: 2, total: 4 }));
    press({ key: 'Enter', shiftKey: true }, input()!);
    press({ key: 'Enter', shiftKey: true }, input()!);
    equal(count(), t('find.count', { current: 4, total: 4 }), 'Shift+Enter goes back, and from the first to the last');
    ok(thread.scrollTop > 0, 'the last match is brought into view at once, with no smooth scroll in a thread without motion');
    equal(thread.innerHTML, before, 'finding rewrote no message');
    press({ key: 'Enter' }, input()!);
    equal(count(), t('find.count', { current: 1, total: 4 }), 'and Enter goes from the last to the first');
    type('zzz');
    equal(count(), t('find.none'));
    ok(document.querySelector<HTMLButtonElement>('.find button')!.disabled, 'with no match there is nowhere to go');
    equal(marks('find'), 0, 'nothing stays painted');
    type('cafe');
    // An answer being written adds text; the matches follow it and keep the current one.
    press({ key: 'Enter' }, input()!);
    thread.querySelector('.md')!.insertAdjacentHTML('beforeend', '<p>one more cafe</p>');
    await until(() => count() === t('find.count', { current: 2, total: 5 }));
    press({ key: 'Escape' }, input()!);
    await settle(300);
    equal(bar(), null, 'closing takes the bar out of the page');
    equal(marks('find') + marks('find-now'), 0, 'and the paint');
    equal(document.activeElement, opener, 'focus goes back to where it was');
  } finally {
    if (page === undefined) delete document.documentElement.dataset.motion;
    else document.documentElement.dataset.motion = page;
    dispose();
    host.remove();
  }
});

check('find: a word selected in the thread fills the box, Esc closes before anything under it, and a bar that is not enabled stays shut', async () => {
  const { host, thread } = stage();
  const [enabled, setEnabled] = createSignal(true);
  const dispose = render(() => createComponent(Find, { get enabled() { return enabled(); }, root: () => thread }), host);
  try {
    const paragraph = thread.querySelector<HTMLElement>('.md p')!;
    getSelection()!.selectAllChildren(paragraph);
    ctrlF();
    await settle();
    equal(input()?.value, paragraph.textContent, 'the selected text is what is looked for');
    ctrlF();
    await settle();
    equal(document.querySelectorAll('.find').length, 1, 'Ctrl+F again only focuses the box');
    let heard = 0;
    const listen = (e: KeyboardEvent) => e.key === 'Escape' && heard++;
    window.addEventListener('keydown', listen);
    press({ key: 'Escape' }, document.body);
    window.removeEventListener('keydown', listen);
    equal(heard, 0, 'Esc stops at the bar, so it does not also close the layers under it');
    await settle(300);
    equal(bar(), null);
    getSelection()!.removeAllRanges();
    setEnabled(false);
    equal(ctrlF(), false, 'the browser\'s find is stopped here too');
    await settle();
    equal(bar(), null, 'no conversation open, no bar');
    setEnabled(true);
    ctrlF();
    await settle();
    ok(bar());
    setEnabled(false);
    await settle(300);
    equal(bar(), null, 'leaving the conversation closes the bar');
  } finally {
    dispose();
    host.remove();
  }
});

check('find: the browser find shows nowhere in the window, with or without the bar on screen', () => {
  installContextMenu();
  equal(ctrlF(), false, 'Settings and the first run have no bar, and still no browser find');
  equal(press({ key: 'a', ctrlKey: true }), true, 'other shortcuts are left alone');
});

check('find: search keeps the first highlighted match clear of its bar at the top', async () => {
  const { host, thread } = stage();
  host.className = 'main';
  host.style.cssText = 'position:fixed;left:0;top:36px;width:720px;height:444px';
  thread.style.cssText = 'position:absolute;inset:52px 0 108px;overflow-y:auto;--thread-tail:0px';
  thread.querySelector('.you')!.textContent = 'Sky at the start';
  const dispose = render(() => createComponent(Find, { enabled: true, root: () => thread }), host);
  try {
    ctrlF();
    await settle();
    type('sky');
    await until(() => marks('find-now') === 1);
    const current = [...CSS.highlights.get('find-now')!][0]! as Range;
    equal(current.toString(), 'Sky', 'the first logical match is current');
    equal(thread.scrollTop, 0, 'the first match has no earlier scroll position available');
    const overlay = bar()!.getBoundingClientRect();
    const rects = [...current.getClientRects()];
    ok(rects.some((r) => r.width > 0 && r.height > 0), 'the current match has a rendered text box');
    ok(rects.every((r) => r.bottom <= overlay.top || r.top >= overlay.bottom
      || r.right <= overlay.left || r.left >= overlay.right), 'the current match must not sit under its own search bar');
  } finally {
    dispose();
    host.remove();
  }
});

check('find: search keeps its count and width while leaving but releases highlights immediately', async () => {
  const { host, thread } = stage('full');
  const dispose = render(() => createComponent(Find, { enabled: true, root: () => thread }), host);
  try {
    ctrlF();
    await settle();
    type('cafe');
    await until(() => marks('find-now') === 1);
    await settle(300);
    const before = { count: count(), width: bar()!.getBoundingClientRect().width };
    press({ key: 'Escape' });
    ok(bar()?.classList.contains('leaving'), 'the search bar is still present during its own exit');
    equal(CSS.highlights.has('find'), false, 'text ranges are released as soon as the search closes');
    equal(CSS.highlights.has('find-now'), false);
    equal(count(), before.count, 'the exiting bar keeps the count the person just saw');
    equal(bar()!.getBoundingClientRect().width, before.width, 'clearing results must not widen the exiting bar');
    await until(() => !bar());
  } finally {
    dispose();
    host.remove();
  }
});

check('find: the bar slides down from above and fades, leaves with a shorter exit, and stays still in reduced motion', async () => {
  const { host, thread } = stage('full');
  const dispose = render(() => createComponent(Find, { enabled: true, root: () => thread }), host);
  try {
    ctrlF();
    await settle(10);
    const entering = bar()!.getAnimations().find((a) => (a as CSSAnimation).animationName === 'find-in');
    ok(entering, 'the bar has an entrance');
    const enter = Number(entering!.effect!.getComputedTiming().duration);
    entering!.pause();
    entering!.currentTime = 0;
    equal(getComputedStyle(bar()!).opacity, '0');
    ok(new DOMMatrix(getComputedStyle(bar()!).transform).m42 < 0, 'it starts above its place');
    entering!.finish();
    press({ key: 'Escape' }, input()!);
    await settle(10);
    const leaving = bar()!;
    ok(leaving.classList.contains('leaving') && leaving.hasAttribute('inert'), 'the bar is still there while it leaves, and takes no input');
    const exit = leaving.getAnimations().find((a) => (a as CSSAnimation).animationName === 'find-out');
    ok(exit && Number(exit.effect!.getComputedTiming().duration) < enter, 'the exit is shorter than the entrance');
    await settle(320);
    equal(bar(), null);
    host.dataset.motion = 'reduced';
    ctrlF();
    await settle(10);
    equal(bar()!.getAnimations().length, 0, 'reduced motion shows it at once');
  } finally {
    dispose();
    host.remove();
  }
});

check('find: the bar shrinks to fit a narrow thread and keeps its buttons whole', async () => {
  const { host, thread } = stage();
  host.style.width = '300px';
  const dispose = render(() => createComponent(Find, { enabled: true, root: () => thread }), host);
  try {
    ctrlF();
    await settle();
    const box = bar()!.getBoundingClientRect();
    const edge = host.getBoundingClientRect();
    ok(box.left >= edge.left && box.right <= edge.right, `the bar sits inside a 300 px thread: ${box.left} to ${box.right}`);
    for (const button of bar()!.querySelectorAll('button')) equal(Math.round(button.getBoundingClientRect().width), 32, 'a button keeps its size');
  } finally {
    dispose();
    host.remove();
  }
});
