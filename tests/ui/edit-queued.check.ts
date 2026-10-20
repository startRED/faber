import { batch, createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState, type Conversation, type QueuedTurn } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { dirtyCount, keep, kept, queuedDrafts } from '../../ui/unsent.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function queue() {
  const keys = ['conversation', 'other'].flatMap((id) => [`faber.unsent.queued-edit.${id}.waiting`, `faber.unsent.${id}`]);
  const stored = keys.map((key) => localStorage.getItem(key));
  keys.forEach((key) => localStorage.removeItem(key));
  const host = document.createElement('div');
  document.body.append(host);
  const item: QueuedTurn = { id: 'waiting', at: 1, text: 'Original words', attachments: ['notes.txt'], choice: { vendor: 'claude' } };
  const [items, setItems] = createSignal([item, { ...item, id: 'next', text: 'Next message' }]);
  const [conversation, setConversation] = createSignal('conversation');
  const recovered: unknown[] = [];
  const dispose = render(() => createComponent(Thread, {
    get conversationId() { return conversation(); }, messages: [], get queued() { return items(); },
    busy: false, from: 0, options: [], onBranch: () => {},
    onQueuedRecovered: (id: string, text: string) => { recovered.push([id, text]); },
  }), host);
  const button = (key: Parameters<typeof t>[0]) => {
    const found = [...host.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.getAttribute('aria-label') === t(key) || b.textContent === t(key));
    ok(found, `button ${key} is present`);
    return found;
  };
  const field = () => {
    const found = host.querySelector<HTMLTextAreaElement>('.mine.queued textarea');
    ok(found, 'the queued editor is present');
    return found;
  };
  const write = (text: string) => {
    field().value = text;
    field().dispatchEvent(new Event('input', { bubbles: true }));
  };
  const key = (key: string, extra: KeyboardEventInit = {}) => {
    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...extra });
    field().dispatchEvent(event);
    return event;
  };
  return { host, item, items, setItems, setConversation, recovered, button, field, write, key, close: () => {
    dispose(); host.remove();
    keys.forEach((key, i) => stored[i] === null ? localStorage.removeItem(key) : localStorage.setItem(key, stored[i]!));
  } };
}

check('edit-queued: Save edits the same id and keeps the order, files and Send now', async () => {
  const before = { edit: api.editQueued, send: api.sendQueued };
  const q = queue();
  const edits: unknown[] = [];
  let sent = '';
  api.editQueued = async (id, turnId, text) => {
    edits.push([id, turnId, text]);
    q.setItems(q.items().map((item) => item.id === turnId ? { ...item, text } : { ...item }));
  };
  api.sendQueued = async (_id, turnId) => { sent = turnId; };
  try {
    q.button('queue.editLabel').click();
    await settle();
    equal(q.field().value, 'Original words');
    equal(document.activeElement, q.field(), 'Edit focuses its text box');
    q.write('Edited words\nSecond line');
    q.button('queue.save').click();
    await settle();
    deepEqual(edits, [['conversation', 'waiting', 'Edited words\nSecond line']]);
    deepEqual(q.items().map((item) => item.id), ['waiting', 'next']);
    deepEqual(q.items()[0].attachments, ['notes.txt']);
    equal(q.host.querySelector('textarea'), null, 'Save closes the editor even after a queue event replaces its objects');
    equal(q.host.querySelector('.queue-text')?.textContent, 'Edited words\nSecond line');
    q.button('queue.sendNowLabel').click();
    await settle();
    equal(sent, 'waiting', 'Send now still uses the edited message id');
  } finally {
    api.editQueued = before.edit; api.sendQueued = before.send;
    q.close();
  }
});

check('edit-queued: Enter saves, Shift+Enter and composition do not, Escape and Cancel discard only the edit', async () => {
  const before = api.editQueued;
  const q = queue();
  const saved: string[] = [];
  api.editQueued = async (_id, _turnId, text) => { saved.push(text); };
  const dirty = dirtyCount();
  try {
    q.button('queue.editLabel').click();
    await settle();
    ok(dirtyCount() > dirty, 'an update cannot close the unsaved editor');
    q.write('Changed');
    equal(q.key('Enter', { shiftKey: true }).defaultPrevented, false, 'Shift+Enter keeps native newline input');
    equal(q.key('Enter', { isComposing: true }).defaultPrevented, false, 'composition cannot submit');
    deepEqual(saved, []);
    q.key('Escape');
    await settle();
    equal(q.host.querySelector('textarea'), null);
    equal(q.items()[0].text, 'Original words');
    equal(dirtyCount(), dirty, 'Cancel clears the unsaved-work marker');
    q.button('queue.editLabel').click();
    q.write(' \n\t');
    q.key('Enter');
    await settle();
    deepEqual(saved, [], 'empty text is never saved');
    equal(q.host.querySelector('[role="alert"]')?.textContent, t('error.queueEmpty'));
    q.button('queue.cancel').click();
    q.button('queue.editLabel').click();
    q.write('Saved by Enter');
    q.key('Enter');
    await settle();
    deepEqual(saved, ['Saved by Enter']);
  } finally {
    api.editQueued = before;
    q.close();
  }
});

check('edit-queued: a refused save appends the edit to the stored conversation draft', async () => {
  const before = api.editQueued;
  try {
    for (const failure of ['error.queueChanged', new Error('error.queueChanged'), { key: 'error.queueChanged' }]) {
      api.editQueued = async () => { throw failure; };
      const q = queue();
      try {
        keep('conversation', 'Already drafted');
        q.button('queue.editLabel').click();
        q.write('Do not lose these words');
        const field = q.field();
        q.setItems(q.items().map((item) => ({ ...item, sending: true })));
        await settle();
        equal(q.field(), field, 'queue events keep the same editor by id');
        q.button('queue.save').click();
        await settle();
        equal(q.host.querySelector('textarea'), null, 'the refused editor closes');
        equal(kept('conversation'), 'Already drafted\n\nDo not lose these words');
        equal(kept('queued-edit.conversation.waiting'), '', 'the edit has one surviving owner');
        deepEqual(q.recovered, [['conversation', 'Already drafted\n\nDo not lose these words']]);
      } finally { q.close(); }
    }
  } finally { api.editQueued = before; }
});

check('edit-queued: Escape cancels from the text box, Save or Cancel, but never during composition', async () => {
  const q = queue();
  try {
    for (const target of ['text', 'queue.save', 'queue.cancel'] as const) {
      q.button('queue.editLabel').click();
      await settle();
      q.write('Changed');
      const control = target === 'text' ? q.field() : q.button(target);
      control.focus();
      const composing = new KeyboardEvent('keydown', { key: 'Escape', isComposing: true, bubbles: true, cancelable: true });
      control.dispatchEvent(composing);
      equal(composing.defaultPrevented, false);
      ok(q.host.querySelector('.editing'), 'composition Escape leaves the editor open');
      const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
      control.dispatchEvent(escape);
      await settle();
      equal(escape.defaultPrevented, true);
      equal(q.host.querySelector('.editing'), null, 'Escape closes the editor from every focused control');
      equal(document.activeElement, q.button('queue.editLabel'));
      equal(q.items()[0].text, 'Original words');
    }
  } finally { q.close(); }
});

check('edit-queued: Cancel and Escape focus the message box when Edit no longer exists', async () => {
  for (const escape of [false, true]) {
    const q = queue();
    const composer = document.createElement('form');
    composer.className = 'composer';
    const run = document.createElement('button');
    run.className = 'run-btn';
    q.host.prepend(run);
    const field = document.createElement('textarea');
    composer.append(field);
    q.host.append(composer);
    try {
      q.button('queue.editLabel').click();
      await settle();
      q.setItems(q.items().map((item) => ({ ...item, sending: true })));
      if (escape) {
        const save = q.button('queue.save');
        save.focus();
        save.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      } else q.button('queue.cancel').click();
      await settle();
      equal(q.host.querySelector('.editing'), null);
      equal(document.activeElement, field, 'focus lands on a surviving message box');
    } finally { q.close(); }
  }
});

check('edit-queued: a message already handed to the CLI offers no Edit', () => {
  const q = queue();
  try {
    q.setItems(q.items().map((item) => ({ ...item, sending: true })));
    equal(q.host.querySelector(`button[aria-label="${t('queue.editLabel')}"]`), null);
  } finally { q.close(); }
});

check('edit-queued: changing conversations keeps each editor with its owner and restores its words', async () => {
  const before = api.editQueued;
  const q = queue();
  const saved: unknown[] = [];
  api.editQueued = async (...args) => { saved.push(args); };
  try {
    q.button('queue.editLabel').click();
    q.write('Words for the first conversation');
    const original = q.items();
    batch(() => {
      q.setConversation('other');
      // Even a reused message id cannot carry its editor into another conversation.
      q.setItems([{ ...q.item, text: 'Other conversation' }]);
    });
    await settle();
    equal(q.host.querySelector('textarea'), null, 'the first conversation editor does not appear in the second');
    q.button('queue.editLabel').click();
    q.write('Words for the other conversation');
    q.button('queue.save').click();
    await settle();
    deepEqual(saved, [['other', 'waiting', 'Words for the other conversation']]);
    batch(() => { q.setConversation('conversation'); q.setItems(original); });
    await settle();
    equal(q.field().value, 'Words for the first conversation', 'returning restores the unsaved edit');
    q.button('queue.save').click();
    await settle();
    deepEqual(saved, [
      ['other', 'waiting', 'Words for the other conversation'],
      ['conversation', 'waiting', 'Words for the first conversation'],
    ]);
  } finally { api.editQueued = before; q.close(); }
});

check('edit-queued: a late Save cannot clear newer words after switching away and back', async () => {
  const before = api.editQueued;
  const q = queue();
  let finish!: () => void;
  api.editQueued = () => new Promise<void>((resolve) => { finish = resolve; });
  const original = q.items();
  const go = (id: string) => batch(() => {
    q.setConversation(id);
    q.setItems(id === 'conversation' ? original : [{ ...q.item, text: 'Other conversation' }]);
  });
  try {
    q.button('queue.editLabel').click();
    q.write('Earlier saved words');
    q.button('queue.save').click();
    await settle();
    go('other');
    await settle();
    go('conversation');
    await settle();
    equal(q.field().value, 'Earlier saved words');
    q.write('Newer unsaved words');
    finish();
    await settle();
    equal(q.field().value, 'Newer unsaved words');
    go('other');
    await settle();
    go('conversation');
    await settle();
    equal(q.field().value, 'Newer unsaved words', 'a disposed editor cannot erase a newer draft');
    q.button('queue.cancel').click();
  } finally { finish?.(); api.editQueued = before; q.close(); }
});

async function appQueue(existing = '', waiting = true) {
  const id = 'queued-recovery-conversation';
  const keys = ['faber.place', `faber.unsent.${id}`, `faber.unsent.queued-edit.${id}.waiting`];
  const stored = keys.map((key) => localStorage.getItem(key));
  keys.forEach((key) => localStorage.removeItem(key));
  keep(id, existing);
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id }));
  const c: Conversation = {
    id, title: 'Queued recovery', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {},
    messages: [{ id: 'user', role: 'user', text: 'Work', at: 0 }, { id: 'answer', role: 'assistant', text: 'Working', at: 1, status: 'streaming', vendor: 'claude' }],
    queued: waiting ? [{ id: 'waiting', at: 2, text: 'Original words', choice: { vendor: 'claude' } }] : [],
  };
  const initial = {
    conversations: [{ id, title: c.title, area: c.area, updatedAt: 1 }], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test',
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
  } as AppState;
  const before = { open: api.open, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, editQueued: api.editQueued };
  api.open = async () => c;
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.editQueued = async () => { throw new Error('error.queueChanged'); };
  const host = document.createElement('div');
  document.body.append(host);
  let hear!: (event: AppEvent) => void;
  const mount = () => render(() => createComponent(App, { initial, inbox: {
    take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; },
  } }), host);
  let dispose = mount();
  const ready = () => new Promise((resolve) => setTimeout(resolve, 30));
  await ready();
  const box = () => {
    const field = host.querySelector<HTMLTextAreaElement>('.composer textarea');
    ok(field, 'the conversation message box is present');
    return field;
  };
  const notices = () => [...host.querySelectorAll('[role="status"]')].filter((el) => el.textContent === t('queue.editRecovered'));
  return { id, c, host, box, notices, ready,
    gone: () => { c.queued = []; hear({ type: 'queue', conversationId: id, queued: [] }); },
    remount: async () => { dispose(); dispose = mount(); await ready(); },
    close: () => {
      dispose(); host.remove(); Object.assign(api, before);
      keys.forEach((key, i) => stored[i] === null ? localStorage.removeItem(key) : localStorage.setItem(key, stored[i]!));
    },
  };
}

check('edit-queued: refused or vanished edits return to the live message box once and survive recreating the app', async () => {
  for (const refused of [false, true]) {
    for (const existing of ['', 'Existing draft\nwith a second line']) {
      const app = await appQueue(existing);
      try {
        app.host.querySelector<HTMLButtonElement>(`button[aria-label="${t('queue.editLabel')}"]`)!.click();
        const edit = app.host.querySelector<HTMLTextAreaElement>('.mine.queued textarea');
        ok(edit);
        edit.value = 'Recovered edit\nSecond line';
        edit.dispatchEvent(new Event('input', { bubbles: true }));
        if (refused) app.host.querySelector<HTMLFormElement>('.mine.queued form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
        else app.gone();
        await app.ready();
        const expected = existing ? `${existing}\n\nRecovered edit\nSecond line` : 'Recovered edit\nSecond line';
        equal(app.host.querySelector('.mine.queued textarea'), null, 'no inaccessible editor is retained');
        equal(app.box().value, expected, 'the live message box preserves both drafts');
        equal(kept(app.id), expected, 'the composer draft is stored for a restart');
        equal(document.activeElement, app.box(), 'keyboard focus returns to the message box');
        equal(app.notices().length, 1, 'one calm notice explains where the edit went');
        app.gone();
        await app.ready();
        equal(app.box().value, expected, 'another queue event cannot append the edit twice');
        await app.remount();
        equal(app.box().value, expected, 'the words survive rebuilding Thread and the whole app');
        equal(app.notices().length, 0, 'a consumed recovery notice is not replayed');
        deepEqual(queuedDrafts(app.id), [], 'no orphaned queue-editor keys remain');
      } finally { app.close(); }
    }
  }
});

check('edit-queued: an orphaned saved edit is restored after a restart without an editor registry', async () => {
  const app = await appQueue('Other words already drafted', false);
  try {
    keep(`queued-edit.${app.id}.waiting`, 'Words saved before the restart');
    await app.remount();
    equal(app.box().value, 'Other words already drafted\n\nWords saved before the restart');
    equal(app.host.querySelector('.mine.queued'), null, 'recovery needs no queued row');
    equal(app.notices().length, 1);
    equal(document.activeElement, app.box());
    await app.remount();
    equal(app.box().value, 'Other words already drafted\n\nWords saved before the restart');
    equal(app.notices().length, 0);
  } finally { app.close(); }
});
