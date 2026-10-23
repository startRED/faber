import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { forgetDraft, keep, keepAttachments, kept } from '../../ui/unsent.ts';
import { check, equal, ok } from './check.ts';

const look = {
  vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
  appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
};

check('save error: a conversation the engine could not write shows in the same toast as a refused command', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const initial = { conversations: [], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  try {
    await settle();
    equal(host.querySelector('.toast'), null, 'nothing is shown while every save works');
    hear({ type: 'saveFailed', key: 'error.chat.saveFailed' });
    await settle();
    equal(host.querySelector('.toast span')?.textContent, t('error.chat.saveFailed'), 'the toast says what the engine key says');
    ok(host.querySelector('.toast[role="alert"]'), 'it is the one alert toast, not a second one');
    host.querySelector<HTMLButtonElement>('.toast button')!.click();
    await settle();
    equal(host.querySelector('.toast'), null, 'dismissing it closes it');
    hear({ type: 'saveFailed', key: 'error.chat.saveFull' });
    await settle();
    equal(host.querySelector('.toast span')?.textContent, t('error.chat.saveFull'), 'a full disk has its own words');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
  }
});

check('save error: a failed draft removal is recovered after reopening and can be retried from the existing toast', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 40));
  const key = 'check-removed-conversation';
  const host = document.createElement('div');
  document.body.append(host);
  const before = { state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const initial = { conversations: [], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  const open = IDBFactory.prototype.open;
  let dispose = () => {};
  try {
    ok(await keepAttachments(key, { images: [], files: [{ name: 'private.pdf', base64: 'AAE=' }], inlined: [] }));
    // A previous process stopped after recording the deletion, before IndexedDB could remove the bytes.
    localStorage.setItem(`faber.unsent-attachments.${key}`, 'remove');
    keep(key, 'A new text draft written after the attachments were removed');
    IDBFactory.prototype.open = () => { throw new Error('storage unavailable'); };
    api.state = async () => initial;
    api.warm = async () => {}; api.routeDraft = async () => {};
    api.setClock = async () => {}; api.setShellTexts = async () => {};
    dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
    await settle();
    equal(host.querySelector('.toast span')?.textContent, t('composer.draftRemoveFailed'), 'startup reports the incomplete deletion');
    equal(localStorage.getItem(`faber.unsent-attachments.${key}`), 'remove', 'failure retains the durable deletion intent');
    const retry = [...host.querySelectorAll<HTMLButtonElement>('.toast button')].find((button) => button.textContent === t('changes.retry'));
    ok(retry, 'the existing toast offers a keyboard-operable retry');
    IDBFactory.prototype.open = open;
    retry!.click();
    await settle();
    equal(localStorage.getItem(`faber.unsent-attachments.${key}`), null, 'successful retry removes the durable marker');
    equal(kept(key), 'A new text draft written after the attachments were removed', 'retrying an attachment removal never discards newer words');
    equal(host.querySelector('.toast'), null, 'the failure clears only after the removal succeeds');
  } finally {
    IDBFactory.prototype.open = open;
    dispose(); host.remove();
    Object.assign(api, before);
    await forgetDraft(key);
  }
});

check('save error: refusing conversation deletion keeps its drafts, while a confirmed deletion retries both saved drafts', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 40));
  const id = 'check-conversation-removal';
  const keys = [id, `aside.${id}`];
  const host = document.createElement('div');
  document.body.append(host);
  const place = localStorage.getItem('faber.place');
  const before = { state: api.state, remove: api.remove, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const initial = { conversations: [{ id, title: 'Remove draft check', area: 'chat', updatedAt: Date.now() }], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  const open = IDBFactory.prototype.open;
  let dispose = () => {};
  let refuse = true;
  try {
    localStorage.setItem('faber.place', JSON.stringify({ area: 'chat' }));
    for (const key of keys) {
      keep(key, 'private unsent words');
      ok(await keepAttachments(key, { images: [], files: [{ name: 'private.pdf', base64: 'AAE=' }], inlined: [] }));
    }
    api.state = async () => initial;
    api.remove = async (requested) => { equal(requested, id); if (refuse) throw new Error('synthetic refusal'); };
    api.warm = async () => {}; api.routeDraft = async () => {};
    api.setClock = async () => {}; api.setShellTexts = async () => {};
    dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
    await settle();
    const remove = () => [...host.querySelectorAll<HTMLButtonElement>('.crow button')].find((button) => button.getAttribute('aria-label') === t('side.delete', { title: 'Remove draft check' }))!;
    ok(remove(), 'the existing conversation deletion control is present');
    remove().click(); await settle();
    remove().click(); await settle();
    for (const key of keys) {
      equal(kept(key), 'private unsent words', 'engine refusal never discards unsent words');
      equal(localStorage.getItem(`faber.unsent-attachments.${key}`), '1', 'engine refusal never discards attachments');
    }
    refuse = false;
    IDBFactory.prototype.open = () => { throw new Error('storage unavailable'); };
    remove().click(); await settle();
    remove().click(); await settle();
    equal(host.querySelector('.toast span')?.textContent, t('composer.draftRemoveFailed'));
    for (const key of keys) equal(localStorage.getItem(`faber.unsent-attachments.${key}`), 'forget', 'confirmed removal keeps its complete deletion intent');
    const retry = [...host.querySelectorAll<HTMLButtonElement>('.toast button')].find((button) => button.textContent === t('changes.retry'));
    ok(retry);
    IDBFactory.prototype.open = open;
    retry!.click(); await settle();
    for (const key of keys) {
      equal(kept(key), '', 'both main and side draft words are removed');
      equal(localStorage.getItem(`faber.unsent-attachments.${key}`), null, 'both draft deletion transactions completed');
    }
  } finally {
    IDBFactory.prototype.open = open;
    dispose(); host.remove();
    Object.assign(api, before);
    if (place === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', place);
    for (const key of keys) await forgetDraft(key);
  }
});
