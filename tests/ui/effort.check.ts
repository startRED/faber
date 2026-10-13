import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { App } from '../../ui/App.tsx';
import { Composer } from '../../ui/Composer.tsx';
import { api, type AppState, type Choice, type Conversation } from '../../ui/api.ts';
import { lang, t, useLanguage } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

check('effort: only a manual model shows low, medium and high; medium is the default and changes are for the next message', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:500px';
  root.dataset.motion = 'none';
  document.body.append(root);
  const [choice, setChoice] = createSignal<Choice>({ vendor: 'auto' });
  let warms = 0;
  let sends = 0;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [{ label: 'Auto', choice: { vendor: 'auto' } }, { label: 'Haiku', choice: { vendor: 'claude', model: 'haiku' } }],
    get choice() { return choice(); }, busy: true, nextMessage: true, placeholder: '',
    onChoice: setChoice, onSend: () => { sends++; }, onWarm: () => { warms++; }, onStop: () => {}, onPause: () => {},
  }), root);
  try {
    await settle();
    equal(root.querySelector('.effort'), null);
    const model = root.querySelector<HTMLSelectElement>('.choose select')!;
    model.value = '1';
    model.dispatchEvent(new Event('change', { bubbles: true }));
    const effort = root.querySelector<HTMLSelectElement>('.effort select')!;
    equal(effort.value, 'medium');
    deepEqual([...effort.options].map((option) => option.value), ['low', 'medium', 'high']);
    effort.value = 'high';
    effort.dispatchEvent(new Event('change', { bubbles: true }));
    equal(choice().effort, 'high');
    equal(root.querySelector('.effort-hint')?.textContent, t('effort.nextMessage'));
    equal(warms, 0, 'changing the choice alone never launches a session');
    equal(sends, 0);
    model.value = '0';
    model.dispatchEvent(new Event('change', { bubbles: true }));
    equal(root.querySelector('.effort'), null);
    equal(root.querySelector('.effort-hint'), null);
    setChoice({ vendor: 'claude', model: 'haiku' });
    equal(root.querySelector<HTMLSelectElement>('.effort select')!.value, 'medium', 'a legacy manual choice defaults to medium');
  } finally {
    dispose();
    root.remove();
  }
});

check('effort: choices restore independently; review 8: successful deletion removes only that conversation choice', async () => {
  const language = lang();
  const keys = ['faber.place', 'faber.choice.effort-one', 'faber.choice.effort-two'];
  const stored = keys.map((key) => localStorage.getItem(key));
  const originalOpen = api.open;
  const originalClock = api.setClock;
  const originalRemove = api.remove;
  const conversations: Conversation[] = ['effort-one', 'effort-two'].map((id) => ({
    id, title: id, area: 'chat', sessions: {}, createdAt: 0, updatedAt: 0,
    choice: id === 'effort-one' ? { vendor: 'claude', model: 'haiku', effort: 'high' } : { vendor: 'auto' },
    messages: [{ id: 'question', role: 'user', text: 'A question', at: 0 }, { id: 'answer', role: 'assistant', text: 'A final answer', at: 1, status: 'done', effort: 'medium' }],
  }));
  const effects = { ambient: false, texture: false, glow: false };
  const initial: AppState = {
    vendors: ['claude'], models: [], quotas: [], projects: [], version: '0.0.0', running: { revision: 0, items: [] },
    conversations: conversations.map(({ id, title, area, updatedAt }) => ({ id, title, area, updatedAt })),
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: {
      vars: themes.css.base, scheme: 'dark', motion: 'none', effects, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects },
    },
  };
  api.open = async (id) => conversations.find((c) => c.id === id) ?? null;
  api.setClock = async () => {};
  for (const key of keys.slice(1)) localStorage.removeItem(key);
  try {
    for (const id of ['effort-one', 'effort-two', 'effort-one', undefined]) {
      localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id }));
      const root = document.createElement('div');
      root.style.cssText = 'position:fixed;inset:0';
      root.dataset.motion = 'none';
      for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
      document.body.append(root);
      const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
      try {
        await settle();
        const effort = root.querySelector<HTMLSelectElement>('.composer .effort select');
        if (id === 'effort-one') {
          const expected = localStorage.getItem('faber.choice.effort-one') ? 'low' : 'high';
          equal(effort?.value, expected);
          ok(root.querySelector('.effort-hint'));
          effort!.value = 'low';
          effort!.dispatchEvent(new Event('change', { bubbles: true }));
          equal(JSON.parse(localStorage.getItem('faber.choice.effort-one')!).effort, 'low');
        } else {
          equal(effort, null, 'another conversation and a new chat retain Auto');
        }
      } finally {
        dispose();
        root.remove();
      }
    }
    localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: 'effort-one' }));
    localStorage.setItem('faber.choice.effort-two', JSON.stringify({ vendor: 'auto' }));
    let complete!: () => void;
    api.remove = () => new Promise<void>((resolve) => { complete = resolve; });
    const root = document.createElement('div');
    root.style.cssText = 'position:fixed;inset:0';
    root.dataset.motion = 'none';
    document.body.append(root);
    const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
    try {
      await settle();
      const button = root.querySelector<HTMLButtonElement>('.crow .x')!;
      button.click();
      button.click();
      await settle();
      ok(localStorage.getItem('faber.choice.effort-one'), 'a pending deletion keeps the saved choice');
      complete();
      await settle();
      equal(localStorage.getItem('faber.choice.effort-one'), null, 'review 8: successful deletion removes its saved choice');
      ok(localStorage.getItem('faber.choice.effort-two'), 'another conversation keeps its choice');
    } finally {
      dispose();
      root.remove();
    }
  } finally {
    api.open = originalOpen;
    api.setClock = originalClock;
    api.remove = originalRemove;
    useLanguage(language);
    keys.forEach((key, i) => { if (stored[i] === null) localStorage.removeItem(key); else localStorage.setItem(key, stored[i]!); });
  }
});
