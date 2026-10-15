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
  const fresh = (): RecentsView => ({
    folders: [folder('alpha'), folder('beta'), folder('gamma', 'reason.recents.gone'), folder('delta'), folder('epsilon')],
    environments: [{ id: 'env-1', name: 'Work set', folders: [folder('alpha'), folder('beta')] }],
  });
  let view = fresh();
  let failRemoval = false;
  const calls: unknown[][] = [];
  api.recents = async () => ({ folders: [...view.folders], environments: [...view.environments] });
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
  // The removals really remove, so the screen shows the list as it is afterwards.
  api.forgetRecent = async (path) => {
    calls.push(['forget', path]);
    if (failRemoval) throw 'error.recents.saveFailed';
    view = { ...view, folders: view.folders.filter((f) => f.path !== path) };
  };
  api.forgetEnvironment = async (id) => {
    calls.push(['forgetEnvironment', id]);
    if (failRemoval) throw 'error.recents.saveFailed';
    view = { ...view, environments: view.environments.filter((e) => e.id !== id) };
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
  const mount = async (area: 'project' | 'chat' = 'project') => {
    view = fresh();
    failRemoval = false;
    localStorage.setItem('faber.place', JSON.stringify({ area }));
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
      const names = () => [...screen.root.querySelectorAll('.recents section:first-child .rec .n')].map((n) => n.textContent);
      const focused = () => {
        const el = document.activeElement as HTMLElement | null;
        return el?.classList.contains('rec-x') ? el.closest('.rec')?.querySelector('.n')?.textContent : el?.className;
      };
      deepEqual(names(), ['alpha', 'beta', 'delta', 'epsilon'], 'the entry is gone from the list');
      equal(focused(), 'delta', 'focus lands on the entry that took the removed one\'s place');
      screen.root.querySelectorAll<HTMLButtonElement>('.recents section:first-child .rec-x')[3].click();
      await settle();
      deepEqual(names(), ['alpha', 'beta', 'delta']);
      equal(focused(), 'delta', 'removing the last entry moves focus to the new last one');
      screen.root.querySelector<HTMLButtonElement>('section[aria-labelledby="env-title"] .rec-x')!.click();
      await settle();
      deepEqual(calls.at(-1), ['forgetEnvironment', 'env-1']);
      equal(screen.root.querySelector('section[aria-labelledby="env-title"]'), null, 'the environment is gone');
      equal(document.activeElement, screen.root.querySelector('.recents'), 'with no entry left in its list, focus stays in the block');
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
    // In Settings, Escape closes only the menu and gives focus back; a second Escape closes Settings. A failed removal is said.
    calls.length = 0;
    screen = await mount('chat');
    try {
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: ',', ctrlKey: true, bubbles: true }));
      await settle();
      const opener = screen.root.querySelector<HTMLButtonElement>('.recents .rec-x')!;
      ok(opener, 'Settings lists the recent folders');
      opener.focus();
      screen.root.querySelector<HTMLElement>('.recents .rec')!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await settle();
      (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      await settle();
      equal(screen.root.querySelector('.rec-menu'), null, 'Escape closes the menu');
      ok(screen.root.querySelector('.recents'), 'Settings is still open');
      equal(document.activeElement, opener, 'focus is back where the menu opened');
      failRemoval = true;
      opener.click();
      await settle();
      ok(screen.root.querySelector('.toast')?.textContent?.includes(t('error.recents.saveFailed')), 'the failed removal is shown');
      (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      await settle();
      equal(screen.root.querySelector('.recents'), null, 'a second Escape closes Settings');
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
