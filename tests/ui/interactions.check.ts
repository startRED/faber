import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type Conversation } from '../../ui/api.ts';
import { lang, useLanguage } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

check('interaction: the running column animates size on open and close, and reduced motion is instant', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:1200px;height:600px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = '<div class="shell" style="height:100%"><aside class="side"></aside><main class="main"></main><aside class="runcol"></aside></div>';
  document.body.append(root);
  const shell = root.firstElementChild as HTMLElement;
  const main = root.querySelector('main')!;
  const halfway = () => {
    const animations = [...shell.getAnimations(), ...main.getAnimations()];
    for (const animation of animations) {
      animation.pause();
      animation.currentTime = Number(animation.effect!.getComputedTiming().duration) / 2;
    }
    return animations;
  };
  const finish = async (animations: Animation[]) => {
    for (const animation of animations) animation.finish();
    await Promise.all(animations.map((animation) => animation.finished));
  };
  try {
    root.dataset.motion = 'full';
    await settle(30);
    const closed = main.getBoundingClientRect().width;
    shell.classList.add('docked');
    await settle(20);
    const entering = halfway();
    const opening = main.getBoundingClientRect().width;
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
      ok(shell.getAnimations().length > 0, 'opening runs a grid transition');
      ok(opening < closed && opening > closed - 352, `opening passes through intermediate sizes: ${closed} to ${opening}`);
    }
    await finish(entering);
    const open = main.getBoundingClientRect().width;
    ok(Math.abs(closed - open - (352 - parseFloat(getComputedStyle(main).getPropertyValue('--stage-inset')))) < 1);
    shell.classList.remove('docked');
    await settle(20);
    const exiting = halfway();
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
      ok(shell.getAnimations().length > 0, 'closing runs a grid transition');
      const closing = main.getBoundingClientRect().width;
      ok(closing > open && closing < closed, 'closing passes through intermediate sizes');
    }
    await finish(exiting);
    root.dataset.motion = 'reduced';
    shell.classList.add('docked');
    equal(main.getBoundingClientRect().width, open, 'reduced opening changes size immediately');
    equal(shell.getAnimations().length, 0);
    shell.classList.remove('docked');
    equal(main.getBoundingClientRect().width, closed, 'reduced closing changes size immediately');
    equal(shell.getAnimations().length, 0);
  } finally {
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
