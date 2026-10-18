import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { App } from '../../ui/App.tsx';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { api, type AppEvent, type AppState, type Conversation, type Message, type QueuedTurn } from '../../ui/api.ts';
import { failureText, t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

check('steer: composer stays usable while busy and sends mid-turn draft', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:500px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);

  let sent = '';
  let fill!: (text: string) => void;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'codex' }, busy: true, placeholder: 'Write here',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onSend: (draft) => { sent = draft.text; }, onReady: (ready) => { fill = ready; },
  }), root);

  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    await settle();
    const field = root.querySelector('textarea')!;
    ok(field, 'textarea is present');
    equal(field.disabled, false, 'textarea is usable while busy');
    const stopBtn = root.querySelector<HTMLButtonElement>('button.stop')!;
    ok(stopBtn, 'stop button is present while busy');

    fill('Mid-turn message');
    await settle();

    const sendBtn = root.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    ok(sendBtn, 'send button appears when draft has content while busy');
    equal(sendBtn.disabled, false, 'send button is enabled when content is typed');

    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(sent, 'Mid-turn message', 'submitting while busy sends draft');
    equal(field.value, '', 'field is cleared after sending');
  } finally {
    dispose();
    root.remove();
  }
});

check('steer: thread renders queued turns with badge and remove/sendNow actions', async () => {
  const root = document.createElement('div');
  document.body.append(root);

  let removedId = '';
  let sentId = '';
  const originalRemove = api.removeQueued;
  const originalSend = api.sendQueued;
  api.removeQueued = async (_convId: string, turnId: string) => { removedId = turnId; };
  api.sendQueued = async (_convId: string, turnId: string) => { sentId = turnId; };

  const message: Message = {
    id: 'm1', role: 'assistant', text: 'Working...', at: 0, status: 'streaming', vendor: 'codex',
  };
  const [busy, setBusy] = createSignal(true);
  const queuedItem: QueuedTurn = {
    id: 'q1', at: 1000, text: 'Queued turn test', images: [], attachments: [], choice: { vendor: 'codex' },
  };

  const dispose = render(() => createComponent(Thread, {
    conversationId: 'conv-test',
    messages: [message],
    get queued() { return [queuedItem]; },
    get busy() { return busy(); },
    from: 0,
    options: [],
    onBranch: () => {},
  }), root);

  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    await settle();
    const queuedEl = root.querySelector('.mine.queued')!;
    ok(queuedEl, 'queued turn is rendered');
    const badge = queuedEl.querySelector('.queue-badge')!;
    ok(badge, 'queue badge is present');
    equal(badge.textContent, t('queue.label'), 'badge shows localized label');
    ok(queuedEl.textContent?.includes('Queued turn test'), 'queued text is shown');

    // While busy: true, remove button is visible, but sendNow is not
    const removeBtn = queuedEl.querySelector<HTMLButtonElement>('button[aria-label="' + t('queue.removeLabel') + '"]')!;
    ok(removeBtn, 'remove button is present');
    const sendNowBefore = queuedEl.querySelector<HTMLButtonElement>('button[aria-label="' + t('queue.sendNowLabel') + '"]');
    equal(sendNowBefore, null, 'sendNow button is not shown while turn is busy');

    removeBtn.click();
    await settle();
    equal(removedId, 'q1', 'clicking remove triggers api.removeQueued');

    // When busy becomes false:
    setBusy(false);
    await settle();
    const sendNowBtn = queuedEl.querySelector<HTMLButtonElement>('button[aria-label="' + t('queue.sendNowLabel') + '"]')!;
    ok(sendNowBtn, 'sendNow button appears when turn is idle');
    sendNowBtn.click();
    await settle();
    equal(sentId, 'q1', 'clicking sendNow triggers api.sendQueued');
  } finally {
    api.removeQueued = originalRemove;
    api.sendQueued = originalSend;
    dispose();
    root.remove();
  }
});

check('steer: a message already handed to the running answer cannot be removed', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const message: Message = { id: 'm1', role: 'assistant', text: 'Working...', at: 0, status: 'streaming', vendor: 'codex' };
  const queued: QueuedTurn[] = [
    { id: 'wait', at: 1, text: 'Waits for the next turn', choice: { vendor: 'codex' } },
    { id: 'sent', at: 2, text: 'Already in the turn', choice: { vendor: 'codex' }, sending: true },
  ];
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'conv-test', messages: [message], queued, busy: true, from: 0, options: [], onBranch: () => {},
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    await settle();
    const bubbles = [...root.querySelectorAll('.mine.queued')];
    equal(bubbles.length, 2, 'both queued messages are drawn');
    const remove = (bubble: Element) => bubble.querySelector('button[aria-label="' + t('queue.removeLabel') + '"]');
    ok(remove(bubbles[0]), 'a message waiting for the next turn can be removed');
    equal(remove(bubbles[1]), null, 'a message already in the running turn cannot');
  } finally {
    dispose();
    root.remove();
  }
});

check('steer: a rejected Remove or Send now reaches onError instead of vanishing', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const before = { remove: api.removeQueued, send: api.sendQueued };
  const refusal = new Error('error.noSubscription');
  api.removeQueued = async () => { throw refusal; };
  api.sendQueued = async () => { throw refusal; };
  const errors: unknown[] = [];
  const message: Message = { id: 'm1', role: 'assistant', text: 'Done', at: 0, status: 'done', vendor: 'codex' };
  const queued: QueuedTurn[] = [{ id: 'q1', at: 1, text: 'Waiting', choice: { vendor: 'codex' } }];
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'conv-test', messages: [message], queued, busy: false, from: 0, options: [], onBranch: () => {},
    onError: (err: unknown) => { errors.push(err); },
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    await settle();
    const bubble = root.querySelector('.mine.queued')!;
    bubble.querySelector<HTMLButtonElement>('button[aria-label="' + t('queue.removeLabel') + '"]')!.click();
    await settle();
    deepEqual(errors, [refusal], 'a refused Remove is reported');
    bubble.querySelector<HTMLButtonElement>('button[aria-label="' + t('queue.sendNowLabel') + '"]')!.click();
    await settle();
    deepEqual(errors, [refusal, refusal], 'a refused Send now is reported');
  } finally {
    Object.assign(api, { removeQueued: before.remove, sendQueued: before.send });
    dispose();
    root.remove();
  }
});

check('steer: the screen shows why Remove failed, and a message written during an answer is never put back in the box', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { open: api.open, send: api.send, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, removeQueued: api.removeQueued };
  const storage = localStorage.getItem('faber.place');
  const answer: Message = { id: 'answer', role: 'assistant', text: 'Counting', at: 1, status: 'streaming', vendor: 'codex' };
  const c: Conversation = {
    id: 'steer-app', title: 'Test', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {},
    messages: [{ id: 'question', role: 'user', text: 'Count', at: 0 }, answer],
    queued: [{ id: 'q1', at: 2, text: 'Already waiting', choice: { vendor: 'codex' } }],
  };
  const initial = { conversations: [{ id: c.id, title: c.title, area: c.area, updatedAt: 1 }], projects: [], vendors: ['codex'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test',
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } }
  } as AppState;
  const sent: string[] = [];
  api.open = async () => c;
  api.send = async (request) => { sent.push(request.text); return c.id; };
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.removeQueued = async () => { throw new Error('error.noSubscription'); };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: c.id }));
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  try {
    await settle();
    host.querySelector<HTMLButtonElement>('button[aria-label="' + t('queue.removeLabel') + '"]')!.click();
    await settle();
    ok(host.textContent!.includes(failureText(new Error('error.noSubscription'))), 'a refused Remove shows its reason on screen');

    const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = 'Stop at ten';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    host.querySelector('form.composer')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    deepEqual(sent, ['Stop at ten']);
    equal(field.value, '', 'the message left the box');

    // The answer it was written during ends with a plan limit. The message is queued or in the turn, so it is the engine's:
    // putting it back in the box would send it twice.
    hear({ type: 'message', conversationId: c.id, message: { ...answer, status: 'error', error: 'error.quotaOut' } });
    await settle();
    equal(field.value, '', 'a message already queued or steered is not put back in the box');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('steer: a first message whose copy could not be made goes back in the box', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { open: api.open, send: api.send, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const storage = localStorage.getItem('faber.place');
  const answer: Message = { id: 'answer', role: 'assistant', text: '', at: 1, status: 'streaming', vendor: 'claude' };
  const c: Conversation = { id: 'copy-app', title: 'Test', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {}, messages: [], queued: [] };
  const initial = { conversations: [{ id: c.id, title: c.title, area: c.area, updatedAt: 1 }], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test',
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } }
  } as AppState;
  // The engine answers at once with the message recorded and its reply typing; the copy is made behind it.
  api.open = async () => c;
  api.send = async () => { c.messages = [{ id: 'question', role: 'user', text: 'Fix it', at: 0 }, answer]; return c.id; };
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: c.id }));
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  try {
    await settle();
    const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = 'Fix it';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    host.querySelector('form.composer')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(field.value, '', 'the message left the box');
    hear({ type: 'message', conversationId: c.id, message: { ...answer, status: 'error', error: 'error.copy.failed' } });
    await settle();
    equal(field.value, 'Fix it', 'the copy failed after send had answered: the message is back in the box');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});
