import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { animateDetails, animateList, watchMotion } from '../../ui/motion.ts';
import { check, equal, ok } from './check.ts';

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const surface = () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:500px;max-height:100vh;overflow:auto';
  root.dataset.motion = 'full';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  return root;
};

check('review 3: keyboard collapse keeps focus on the project toggle; filtering keeps it in search', async () => {
  const root = surface();
  const dispose = render(() => createComponent(Sidebar, {
    conversations: [{ id: 'chat', title: 'A conversation', area: 'project', projectId: 'project', updatedAt: 0 }],
    projects: [{ id: 'project', name: 'Project', path: 'X:/sandbox' }], vendors: [], version: 'test',
    place: { area: 'project', projectId: 'project', id: 'chat' }, running: [], quotas: {}, now: 0,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
    onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
  }), root);
  try {
    const header = root.querySelector<HTMLButtonElement>('.prow .item')!;
    const row = root.querySelector<HTMLButtonElement>('.proj-chats .item')!;
    row.focus(); equal(document.activeElement, row);
    header.focus(); header.click();
    await settle(260);
    equal(document.activeElement, header);
    ok(root.querySelector<HTMLElement>('.fold')!.inert);
    const search = root.querySelector<HTMLInputElement>('input[type=search]')!;
    search.focus(); search.value = 'missing'; search.dispatchEvent(new Event('input', { bubbles: true }));
    await settle(260);
    equal(document.activeElement, search);
  } finally { dispose(); root.remove(); }
});

check('review 4/13: full-motion growth is laid out at once and opened up by a clip, and capped editing keeps the caret and scroll stable', async () => {
  const root = surface();
  let fill!: (text: string) => void;
  const dispose = render(() => createComponent(Composer, { vendors: ['codex'], options: [], choice: { vendor: 'auto' },
    busy: false, placeholder: '', onChoice: () => {}, onSend: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onReady: (ready) => { fill = ready; } }), root);
  try {
    await settle();
    const field = root.querySelector('textarea')!;
    const initial = field.clientHeight;
    fill('Wrapped prompt. '.repeat(30));
    await settle(45);
    // The field is already at its new height: only the card around it is drawn smaller and opens up, so no layout runs.
    ok(field.clientHeight > initial, 'the field grows at once');
    const card = root.querySelector<HTMLElement>('.composer')!;
    ok(card.getAnimations().some((a) => a.playState === 'running' && (a.effect as KeyframeEffect).getKeyframes().some((k) => 'clipPath' in k)), 'the card opens up with a clip');
    ok(!field.getAnimations().length, 'the field itself is not animated');
    for (let i = 0; i < 6; i++) {
      document.execCommand('insertText', false, ' another word');
      await settle(25);
      ok(field.clientHeight >= initial && field.clientHeight <= innerHeight * 0.4 + 1);
    }
    fill('A prompt line with enough content to wrap.\n'.repeat(100));
    await settle(280);
    field.focus(); field.setSelectionRange(210, 210);
    field.scrollTop = 80;
    const scroll = field.scrollTop;
    for (let i = 0; i < 8; i++) {
      const caret = field.selectionStart;
      ok(document.execCommand('insertText', false, 'x'), 'native editing must actually run');
      await settle(25);
      equal(field.selectionStart, caret + 1);
      ok(Math.abs(field.scrollTop - scroll) < 24, `unexpected scroll jump: ${scroll} → ${field.scrollTop}`);
      ok(field.clientHeight <= innerHeight * 0.4 + 1);
    }
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle(280);
    equal(field.clientHeight, initial);
    equal(field.scrollTop, 0);
    equal(field.value, '');
  } finally { dispose(); root.remove(); }
});

check('review 11: focus entering a disclosure during its exit returns to the summary before hiding', async () => {
  const root = surface();
  root.innerHTML = '<details class="kit disclosure" open><summary>Tools</summary><div><a href="#">Tool details</a></div></details>';
  const dispose = render(() => { animateDetails(root); return [...root.children] as HTMLElement[]; }, root);
  try {
    const summary = root.querySelector('summary')!;
    const link = root.querySelector('a')!;
    summary.focus(); summary.click();
    link.focus(); equal(document.activeElement, link, 'Tab can enter content while an exit still holds details open');
    await settle(260);
    equal(root.querySelector('details')!.open, false);
    equal(document.activeElement, summary, `collapse stranded focus on ${document.activeElement?.tagName}`);
  } finally { dispose(); root.remove(); }
});

check('review 9: reduced motion, resize suppression and cancellation leave no exit ghosts', async () => {
  const root = surface();
  root.innerHTML = '<div style="height:40px">one</div><div style="height:40px">two</div>';
  const width = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
  const originalWidth = innerWidth;
  const dispose = render(() => { watchMotion(); animateList(root); return [...root.children] as HTMLElement[]; }, root);
  try {
    for (const mode of ['reduced', 'none', 'full']) {
      root.dataset.motion = mode;
      if (mode === 'full') {
        Object.defineProperty(window, 'innerWidth', { configurable: true, value: originalWidth - 1 });
        window.dispatchEvent(new Event('resize'));
      }
      root.firstElementChild!.remove(); root.append(document.createElement('div'));
      await settle();
      equal(document.querySelector('.motion-exit'), null);
    }
  } finally {
    Object.defineProperty(window, 'innerWidth', width);
    window.dispatchEvent(new Event('resize'));
    dispose(); root.remove(); await settle(280);
  }
});

check('review 10: message resize listeners are bounded, released on unmount, and not fired by ancestor scrolling', async () => {
  const add = window.addEventListener;
  const remove = window.removeEventListener;
  const rect = HTMLElement.prototype.getBoundingClientRect;
  const listeners = new Set<EventListenerOrEventListenerObject>();
  let reads = 0;
  window.addEventListener = function(type: string, listener: EventListenerOrEventListenerObject, options?: boolean | AddEventListenerOptions) { if (type === 'resize') listeners.add(listener); add.call(this, type, listener, options); };
  window.removeEventListener = function(type: string, listener: EventListenerOrEventListenerObject, options?: boolean | EventListenerOptions) { if (type === 'resize') listeners.delete(listener); remove.call(this, type, listener, options); };
  HTMLElement.prototype.getBoundingClientRect = function() { reads++; return rect.call(this); };
  try {
    for (let round = 0; round < 3; round++) {
      const root = surface(); root.className = 'thread';
      const dispose = render(() => createComponent(Thread, { messages: Array.from<unknown, Message>({ length: 40 }, (_, i) => ({
        id: String(i), role: 'assistant', at: 0, text: 'An answer.', status: 'done',
      })), from: 0, conversationId: 'review', options: [], onBranch: () => {} }), root);
      try {
        await settle(); equal(listeners.size, 0);
        reads = 0; root.dispatchEvent(new Event('scroll'));
        equal(reads, 0, 'a parent scroll does not traverse descendant message listeners');
        const started = performance.now();
        window.dispatchEvent(new Event('resize'));
        ok(reads <= 6, `only the app's outer surfaces may be measured, got ${reads} reads`);
        ok(performance.now() - started < 16, 'all message captures fit one frame');
      } finally { dispose(); root.remove(); }
      equal(listeners.size, 0, 'all listeners leave with their messages');
    }
  } finally { window.addEventListener = add; window.removeEventListener = remove; HTMLElement.prototype.getBoundingClientRect = rect; }
});

check('review 16: cached exit CSS inherits the current theme font, density and custom colors', async () => {
  const root = surface();
  root.innerHTML = '<div class="ai" style="color:var(--ink);font:var(--fs-read)/1.5 var(--font-ui);padding:calc(10px * var(--dens))">old</div>';
  const html = document.documentElement;
  const original = html.getAttribute('style');
  const dispose = render(() => { animateList(root); return [...root.children] as HTMLElement[]; }, root);
  try {
    for (const [color, density] of [['rgb(180, 190, 200)', '1'], ['rgb(210, 180, 160)', '1.2']]) {
      html.style.setProperty('--ink', color!); html.style.setProperty('--dens', density!);
      html.style.setProperty('--fs-read', '16px'); html.style.setProperty('--font-ui', 'monospace');
      const row = root.firstElementChild as HTMLElement;
      row.style.color = 'var(--ink)';
      // The real theme variables live on html, not the fixture's local defaults.
      for (const key of ['ink', 'dens', 'fs-read', 'font-ui']) root.style.removeProperty(`--${key}`);
      const expected = getComputedStyle(row);
      const values = [expected.color, expected.fontFamily, expected.padding];
      row.remove(); await settle();
      const copy = document.querySelector('.motion-exit')?.shadowRoot?.querySelector<HTMLElement>('.ai');
      ok(copy); const actual = getComputedStyle(copy);
      equal(actual.color, values[0]); equal(actual.fontFamily, values[1]); equal(actual.padding, values[2]);
      await settle(260);
      root.append(row); await settle(260);
    }
  } finally { dispose(); root.remove(); if (original === null) html.removeAttribute('style'); else html.setAttribute('style', original); }
});
