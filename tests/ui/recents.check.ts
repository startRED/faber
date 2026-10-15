import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type FolderView, type RecentsView } from '../../ui/api.ts';
import { lang, t, useLanguage } from '../../ui/i18n.ts';
import { shortPath } from '../../ui/Recents.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = (ms = 40) => new Promise((resolve) => setTimeout(resolve, ms));

const folder = (name: string, reason?: string): FolderView => ({ path: `X:/work/${name}`, name, parent: 'X:/work', reason });

check('recents: a long parent path keeps the drive and the last two folders, and short ones stay whole', () => {
  equal(shortPath('E:\\Documentos\\ProjetosIA\\faber'), 'E:\\…\\ProjetosIA\\faber');
  equal(shortPath('/home/erick/work/clients/faber'), '/…/clients/faber');
  equal(shortPath('E:\\work\\faber'), 'E:\\work\\faber');
  equal(shortPath('X:/work'), 'X:/work');
});

check('recents: a recent folder opens as a project in one click, a saved environment starts in one click, and the menu forgets', async () => {
  const stored = localStorage.getItem('faber.place');
  const language = lang();
  const original = { ...api };
  const view: RecentsView = {
    folders: [folder('alpha'), folder('beta'), folder('gamma', 'reason.recents.gone'), folder('delta'), folder('epsilon')],
    environments: [{ id: 'env-1', name: 'Work set', folders: [folder('alpha'), folder('beta')] }],
  };
  const calls: unknown[][] = [];
  api.recents = async () => view;
  api.setClock = async () => {};
  api.openFolders = async (paths) => {
    calls.push(['open', paths]);
    return [{ id: 'p-open', name: 'alpha', path: paths[0] }];
  };
  api.startEnvironment = async (id) => {
    calls.push(['start', id]);
    return [{ id: 'p-env', name: 'alpha', path: 'X:/work/alpha' }];
  };
  api.saveEnvironment = async (name, folders) => {
    calls.push(['save', name, folders]);
  };
  api.forgetRecent = async (path) => {
    calls.push(['forget', path]);
  };
  api.forgetEnvironment = async (id) => {
    calls.push(['forgetEnvironment', id]);
  };
  const effects = { ambient: false, texture: false, glow: false };
  const initial: AppState = {
    vendors: ['claude'], models: [], quotas: [], projects: [], conversations: [], version: '0.0.0', running: { revision: 0, items: [] },
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: {
      vars: themes.css.base, scheme: 'dark', motion: 'none', effects, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects },
    },
  };
  const mount = async () => {
    localStorage.setItem('faber.place', JSON.stringify({ area: 'project' }));
    const root = document.createElement('div');
    root.style.cssText = 'position:fixed;inset:0';
    root.dataset.motion = 'none';
    for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
    document.body.append(root);
    const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
    await settle();
    return { root, done: () => { dispose(); root.remove(); } };
  };
  try {
    useLanguage('en');
    // One click on a recent folder opens it.
    let screen = await mount();
    try {
      const rows = [...screen.root.querySelectorAll<HTMLElement>('.recents section:first-child .rec')];
      equal(rows.length, 5);
      deepEqual(rows.map((r) => r.querySelector('.n')?.textContent), ['alpha', 'beta', 'gamma', 'delta', 'epsilon']);
      equal(rows[0].querySelector('.d')?.textContent, 'X:/work');
      const gone = rows[2].querySelector<HTMLButtonElement>('.rec-main')!;
      ok(gone.disabled, 'a folder that is gone cannot be picked');
      equal(rows[2].querySelector('.d')?.textContent, t('reason.recents.gone'));
      equal(rows[2].querySelector('.rec-sel'), null);
      rows[0].querySelector<HTMLButtonElement>('.rec-main')!.click();
      await settle();
      deepEqual(calls.at(-1), ['open', ['X:/work/alpha']]);
      equal(JSON.parse(localStorage.getItem('faber.place')!).projectId, 'p-open');
    } finally {
      screen.done();
    }
    // Select two folders, name them and save; then start the saved environment.
    calls.length = 0;
    screen = await mount();
    try {
      const rows = [...screen.root.querySelectorAll<HTMLElement>('.recents section:first-child .rec')];
      rows[0].querySelector<HTMLButtonElement>('.rec-sel')!.click();
      rows[3].querySelector<HTMLButtonElement>('.rec-sel')!.click();
      await settle();
      const name = screen.root.querySelector<HTMLInputElement>('.env-tray input')!;
      name.value = 'Pair';
      name.dispatchEvent(new Event('input', { bubbles: true }));
      await settle();
      screen.root.querySelector<HTMLFormElement>('.env-tray')!.requestSubmit();
      await settle();
      deepEqual(calls.at(-1), ['save', 'Pair', ['X:/work/alpha', 'X:/work/delta']]);
      equal(screen.root.querySelector('.env-tray'), null, 'the tray closes after saving');
      const env = screen.root.querySelector<HTMLButtonElement>('section[aria-labelledby="env-title"] .rec-main')!;
      equal(env.querySelector('.n')?.textContent, 'Work set');
      env.click();
      await settle();
      deepEqual(calls.at(-1), ['start', 'env-1']);
      equal(JSON.parse(localStorage.getItem('faber.place')!).projectId, 'p-env');
    } finally {
      screen.done();
    }
    // A keyboard user reaches removal without the menu, also on a folder that is gone, and focus stays in the list.
    calls.length = 0;
    screen = await mount();
    try {
      const buttons = [...screen.root.querySelectorAll<HTMLButtonElement>('.recents section:first-child .rec-x')];
      equal(buttons.length, 5);
      ok(buttons.every((b) => !b.disabled && b.getAttribute('aria-label')?.length), 'every row has a named, enabled remove button');
      buttons[2].click();
      await settle();
      deepEqual(calls.at(-1), ['forget', 'X:/work/gamma']);
      const again = screen.root.querySelectorAll<HTMLElement>('.recents section:first-child .rec-x');
      equal(document.activeElement, again[2], 'focus lands on the entry that took the removed one\'s place');
      screen.root.querySelector<HTMLButtonElement>('section[aria-labelledby="env-title"] .rec-x')!.click();
      await settle();
      deepEqual(calls.at(-1), ['forgetEnvironment', 'env-1']);
      ok(screen.root.contains(document.activeElement), 'focus is still inside the screen');
    } finally {
      screen.done();
    }
    // The right-click menu has one line and forgets the folder; closing it with Escape gives focus back.
    calls.length = 0;
    screen = await mount();
    try {
      const opener = screen.root.querySelector<HTMLButtonElement>('.recents .rec-x')!;
      opener.focus();
      screen.root.querySelector<HTMLElement>('.recents .rec')!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await settle();
      equal(screen.root.querySelector('.rec-menu [role="menuitem"]'), document.activeElement, 'the menu takes focus');
      dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await settle();
      equal(document.activeElement, opener, 'Escape puts focus back where the menu came from');
    } finally {
      screen.done();
    }
    // The right-click menu has one line and forgets the folder.
    calls.length = 0;
    screen = await mount();
    try {
      const row = screen.root.querySelector<HTMLElement>('.recents .rec')!;
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 60, clientY: 80 }));
      await settle();
      const items = screen.root.querySelectorAll<HTMLButtonElement>('.rec-menu [role="menuitem"]');
      equal(items.length, 1);
      equal(items[0].textContent, t('recents.forget'));
      dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await settle();
      equal(screen.root.querySelector('.rec-menu'), null, 'Escape closes the menu');
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 60, clientY: 80 }));
      await settle();
      screen.root.querySelector<HTMLButtonElement>('.rec-menu [role="menuitem"]')!.click();
      await settle();
      deepEqual(calls.at(-1), ['forget', 'X:/work/alpha']);
      equal(screen.root.querySelector('.rec-menu'), null);
    } finally {
      screen.done();
    }
  } finally {
    Object.assign(api, original);
    useLanguage(language);
    if (stored === null) localStorage.removeItem('faber.place');
    else localStorage.setItem('faber.place', stored);
  }
});
