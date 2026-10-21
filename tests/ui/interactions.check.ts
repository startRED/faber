import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type Conversation } from '../../ui/api.ts';
import { animateList } from '../../ui/motion.ts';
import { lang, useLanguage } from '../../ui/i18n.ts';
import { check, equal, finished, ok } from './check.ts';

const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

check('interaction: the running column slides and fades for 120ms while the thread width moves without a jump', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:1200px;height:600px';
  root.dataset.motion = 'full';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = '<div class="shell" style="height:100%"><aside class="side"></aside><main class="main"><header></header><div class="work"><div class="thread"><div class="col">A thread</div></div></div></main></div>';
  document.body.append(root);
  const shell = root.firstElementChild as HTMLElement;
  const main = root.querySelector('main')!;
  const thread = root.querySelector('.thread')!;
  const column = document.createElement('aside');
  column.className = 'runcol';
  const dispose = render(() => { animateList(shell); return [...shell.children] as HTMLElement[]; }, shell);
  const toggle = (open: boolean) => {
    shell.classList.toggle('docked', open);
    if (open) shell.append(column); else column.remove();
  };
  const finish = async () => {
    const ghosts = [...(document.querySelector('.motion-exit')?.shadowRoot?.querySelectorAll<HTMLElement>('.motion-exit') ?? [])];
    const animations = [...main.getAnimations(), ...column.getAnimations(), ...ghosts.flatMap((el) => el.getAnimations())];
    for (const animation of animations) animation.finish();
    await finished(animations);
    await settle(0);
  };
  try {
    await settle(30);
    const closed = thread.getBoundingClientRect().width;
    let open = closed;
    let taken = 0;
    for (const opening of [true, false]) {
      const before = thread.getBoundingClientRect().width;
      toggle(opening);
      await settle(20);
      const animation = main.getAnimations()[0];
      ok(animation, 'the actual stage has a FLIP animation');
      ok(Number(animation.effect!.getComputedTiming().duration) >= 120);
      equal(shell.getAnimations().length, 0, 'no layout animation on the grid');
      animation.pause();
      animation.currentTime = 0;
      ok(Math.abs(thread.getBoundingClientRect().width - before) < 1, 'the first frame retains the previous thread width');
      animation.currentTime = 60;
      const early = thread.getBoundingClientRect().width;
      animation.currentTime = 120;
      const later = thread.getBoundingClientRect().width;
      ok(opening ? later < early : later > early, 'the thread keeps moving through intermediate frames over 120ms');
      if (opening) {
        const entrance = column.getAnimations()[0]!;
        entrance.pause();
        entrance.currentTime = 0;
        equal(getComputedStyle(column).opacity, '0');
        ok(new DOMMatrix(getComputedStyle(column).transform).m41 > 0, 'the column slides in from the right');
      }
      await finish();
      if (opening) [open, taken] = [thread.getBoundingClientRect().width, column.getBoundingClientRect().width];
    }
    // However wide the window leaves the column (100vw in the stylesheet), the thread gives up at least the column's width.
    ok(taken >= 200 && closed - open >= taken - 1, 'the docked column takes its room from the thread');
    root.dataset.motion = 'reduced';
    for (const opening of [true, false]) {
      toggle(opening);
      equal(thread.getBoundingClientRect().width, opening ? open : closed, 'reduced motion changes the thread immediately');
      await settle(20);
      equal(main.getAnimations().length, 0);
      equal(column.getAnimations().length, 0);
      equal(document.querySelector('.motion-exit'), null);
    }
    // An interrupted opening starts its exit at the currently rendered width too.
    root.dataset.motion = 'full';
    toggle(true);
    await settle(20);
    const entering = main.getAnimations()[0];
    if (entering) {
      entering.pause();
      entering.currentTime = 60;
      const before = thread.getBoundingClientRect().width;
      toggle(false);
      await settle(0);
      const exiting = main.getAnimations()[0]!;
      exiting.pause();
      exiting.currentTime = 0;
      ok(Math.abs(thread.getBoundingClientRect().width - before) < 1, 'an interrupted toggle also retains its current width');
    }
    await finish();
  } finally {
    dispose();
    root.remove();
  }
});
check('interaction: both project and conversation breadcrumbs render the single character ›', async () => {
  const stored = localStorage.getItem('faber.place');
  const language = lang();
  const originalOpen = api.open;
  const originalClock = api.setClock;
  const conversation: Conversation = {
    id: 'breadcrumb-check', title: 'A conversation', area: 'project', projectId: 'demo',
    sessions: {}, createdAt: 0, updatedAt: 0, messages: [],
  };
  const effects = { ambient: false, texture: false, glow: false };
  const initial: AppState = {
    vendors: [], models: [], quotas: [], version: '0.0.0', running: { revision: 0, items: [] },
    projects: [{ id: 'demo', name: 'A project', path: 'X:/sandbox' }],
    conversations: [{ id: conversation.id, title: conversation.title, area: 'project', projectId: 'demo', updatedAt: 0 }],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: {
      vars: themes.css.base, scheme: 'dark', motion: 'none', effects, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects },
    },
  };
  api.open = async () => conversation;
  api.setClock = async () => {};
  try {
    for (const id of [undefined, conversation.id]) {
      localStorage.setItem('faber.place', JSON.stringify({ area: 'project', projectId: 'demo', id }));
      const root = document.createElement('div');
      root.style.cssText = 'position:fixed;inset:0';
      root.dataset.motion = 'none';
      for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
      document.body.append(root);
      const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
      try {
        await settle(30);
        const crumb = root.querySelector('.crumb')!;
        equal(crumb.querySelector('.sep')?.textContent?.trim(), '›');
        ok(crumb.textContent?.includes('›'));
        ok(!crumb.textContent?.includes('â€º'));
      } finally {
        dispose();
        root.remove();
      }
    }
  } finally {
    api.open = originalOpen;
    api.setClock = originalClock;
    useLanguage(language);
    if (stored === null) localStorage.removeItem('faber.place');
    else localStorage.setItem('faber.place', stored);
  }
});
