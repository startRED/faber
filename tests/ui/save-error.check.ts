import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState, type Conversation } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { forgetDraft, keep, keepAttachments, kept, release } from '../../ui/unsent.ts';
import { check, equal, ok } from './check.ts';

const look = {
  vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
  appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
};

const removalFinished = async (keys: string[]) => {
  const deadline = performance.now() + 2000;
  while (keys.some((key) => localStorage.getItem(`faber.unsent-attachments.${key}`) !== null) && performance.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 10));
};

check('t520: closing preserves text whose storage write failed, even after its box was released', async () => {
  const hadShell = 'faberShell' in window;
  if (!hadShell) Object.defineProperty(window, 'faberShell', { configurable: true, value: {} });
  const host = document.createElement('div'); document.body.append(host);
  const before = { warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const initial = { conversations: [], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
  const write = Storage.prototype.setItem;
  try {
    Storage.prototype.setItem = () => { throw new Error('disk full'); };
    keep('t520-close', 'Synthetic unsaved words');
    release('t520-close');
    const closing = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(closing);
    equal(closing.defaultPrevented, true, 'the shell must keep the renderer with the only copy alive');
    equal(kept('t520-close'), 'Synthetic unsaved words');
  } finally {
    Storage.prototype.setItem = write;
    keep('t520-close', ''); dispose(); host.remove(); Object.assign(api, before);
    if (!hadShell) Reflect.deleteProperty(window, 'faberShell');
  }
});

check('t520: a delayed refused send restores its words below newer text and retries once', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const oldPlace = localStorage.getItem('faber.place');
  const oldDraft = kept('new-chat-');
  const before = { send: api.send, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const initial = { conversations: [], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  let reject!: (error: unknown) => void;
  let sends = 0;
  api.send = () => { sends++; return new Promise((_resolve, fail) => { reject = fail; }); };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat' })); keep('new-chat-', '');
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
  const input = (text: string) => {
    const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = text; field.dispatchEvent(new Event('input', { bubbles: true }));
  };
  try {
    await new Promise((resolve) => setTimeout(resolve, 40));
    input('Synthetic rejected A'); host.querySelector<HTMLFormElement>('.composer')!.requestSubmit();
    input('Synthetic newer B');
    reject({ key: 'error.chat.saveFailed' });
    await new Promise((resolve) => setTimeout(resolve, 40));
    equal(host.querySelector<HTMLTextAreaElement>('.composer textarea')!.value, 'Synthetic newer B\n\nSynthetic rejected A');
    equal(kept('new-chat-'), 'Synthetic newer B\n\nSynthetic rejected A');
    host.querySelector<HTMLFormElement>('.composer')!.requestSubmit();
    equal(sends, 2, 'one explicit retry makes one additional request');
    reject({ key: 'error.chat.saveFailed' });
    await new Promise((resolve) => setTimeout(resolve, 40));
  } finally {
    dispose(); host.remove(); Object.assign(api, before); keep('new-chat-', oldDraft);
    if (oldPlace === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', oldPlace);
  }
});

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
    await removalFinished([key]);
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
    // Real storage can finish after a rendering tick; retain that delay even on a fast local disk.
    IDBFactory.prototype.open = function (name, version) {
      const request = open.call(this, name, version);
      Object.defineProperty(request, 'onsuccess', { set: (listener: IDBRequest['onsuccess']) => {
        request.addEventListener('success', (event) => setTimeout(() => listener?.call(request, event), 80), { once: true });
      } });
      return request;
    };
    retry!.click(); await removalFinished(keys);
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

for (const eventFirst of [true, false]) {
  check(`save error: a refused send and its save event share one notice (${eventFirst ? 'event first' : 'command first'})`, async () => {
    const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
    const host = document.createElement('div');
    document.body.append(host);
    const before = { state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock,
      setShellTexts: api.setShellTexts, send: api.send };
    const initial = { conversations: [], projects: [], vendors: ['claude'],
      settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
      running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
    } as AppState;
    api.state = async () => initial;
    api.warm = async () => {}; api.routeDraft = async () => {};
    api.setClock = async () => {}; api.setShellTexts = async () => {};
    let hear!: (event: AppEvent) => void;
    let key = 'error.chat.saveFailed';
    api.send = async () => {
      if (eventFirst && key !== 'error.noSubscription') hear({ type: 'saveFailed', key });
      throw eventFirst ? key : { key };
    };
    const dispose = render(() => createComponent(App, { initial,
      inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } },
    }), host);
    let earlier = '';
    try {
      await settle();
      earlier = host.querySelector<HTMLTextAreaElement>('.composer textarea')!.value;
      for (const error of ['error.chat.saveFailed', 'error.chat.saveFull', 'error.noSubscription']) {
        key = error;
        const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
        field.value = 'keep this message';
        field.dispatchEvent(new Event('input', { bubbles: true }));
        field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
        await settle();
        if (!eventFirst && error !== 'error.noSubscription') {
          hear({ type: 'saveFailed', key });
          await settle();
        }
        equal(field.value, 'keep this message', 'the rejected send restores the draft');
        if (error === 'error.noSubscription') {
          equal(host.querySelector('.toast'), null, 'an unrelated failure does not become a save warning');
          ok(host.querySelector('.note.err[role="alert"]'), 'an unrelated failure remains visible');
          hear({ type: 'saveFailed', key: 'error.chat.saveFailed' });
          await settle();
          ok(host.querySelector('.note.err[role="alert"]'), 'a save warning leaves the unrelated failure visible');
          equal(host.querySelectorAll('.toast[role="alert"]').length, 1, 'the save warning still appears once');
        } else {
          equal(host.querySelectorAll('.toast[role="alert"]').length, 1, 'event and command reuse one save notice');
          equal(host.querySelector('.toast span')?.textContent, t(error as 'error.chat.saveFailed'), 'the returned fault keeps its own words');
          equal(host.querySelector('.note.err[role="alert"]'), null, 'there is no duplicate warning under the composer');
          host.querySelector<HTMLButtonElement>('.toast button')!.click();
          await settle();
        }
      }
    } finally {
      const field = host.querySelector<HTMLTextAreaElement>('.composer textarea');
      if (field) { field.value = earlier; field.dispatchEvent(new Event('input', { bubbles: true })); }
      dispose(); host.remove(); Object.assign(api, before);
    }
  });
}

check('save error: accepted research stays sent when its later Settings write fails', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 40));
  const host = document.createElement('div'); document.body.append(host);
  const before = { state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock,
    setShellTexts: api.setShellTexts, send: api.send, open: api.open, commands: api.commands };
  const oldDraft = kept('new-chat-');
  const oldPlace = localStorage.getItem('faber.place');
  const oldChoice = localStorage.getItem('faber.choice.accepted-research');
  const initial = { conversations: [], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  const conversation = { id: 'accepted-research', title: 'Research', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {}, queued: [],
    choice: { vendor: 'claude' }, messages: [
      { id: 'accepted-question', role: 'user', text: '/research compare the choices', at: 1 },
      { id: 'settings-failed', role: 'assistant', text: '', at: 1, status: 'error', error: 'error.settings.saveFailed' },
    ],
  } as Conversation;
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.open = async () => conversation; api.commands = async () => [];
  let sends = 0;
  api.send = async () => { sends++; return conversation.id; };
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
  try {
    await settle();
    const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = '/research compare the choices';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await settle(); await settle();
    equal(sends, 1, 'the accepted intent is not automatically sent a second time');
    equal(host.querySelector<HTMLTextAreaElement>('.composer textarea')!.value, '', 'a later settings failure does not restore an accepted draft');
    ok(host.textContent?.includes(t('error.settings.saveFailed')), 'the saved reply explains the later Settings failure');
    equal(host.querySelectorAll('article.ai').length, 1, 'one accepted intent has one failed reply');
  } finally {
    dispose(); host.remove(); Object.assign(api, before);
    keep('new-chat-', oldDraft);
    for (const [key, value] of [['faber.place', oldPlace], ['faber.choice.accepted-research', oldChoice]] as const) {
      if (value === null) localStorage.removeItem(key); else localStorage.setItem(key, value);
    }
  }
});
