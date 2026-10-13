import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { api, type Message, type QueuedTurn } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

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
