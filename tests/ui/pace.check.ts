import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppState, type Conversation, type Message } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { Composer, type Draft } from '../../ui/Composer.tsx';
import { QuotaCard } from '../../ui/QuotaCard.tsx';
import { useLanguage } from '../../ui/i18n.ts';
import { vendorName } from '../../ui/models.ts';
import { Settings } from '../../ui/Settings.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { check, equal, ok } from './check.ts';

check('pace: quota reasons, manual effort and grouped activity coexist without hiding the answer', () => {
  useLanguage('en');
  for (const kind of ['step', 'fast', 'low'] as const) {
    const host = document.createElement('div');
    const narration = 'Checking the files.\n\n';
    const m = { id: 'reply', role: 'assistant', text: narration + 'Answer', activityEnd: narration.length,
      tools: [{ id: 'check', name: 'Bash', input: { command: 'check' }, ok: true }], effort: 'high',
      at: 1, status: 'done', vendor: 'claude', reason: 'reason.route.code.medium', pace: { kind, vendor: 'claude' } } as Message;
    const dispose = render(() => createComponent(Thread, { conversationId: 'chat', messages: [m], from: 0, options: [], onBranch: () => {} }), host);
    try {
      const normal = host.querySelector<HTMLElement>('.model');
      const pace = host.querySelector('.pace');
      ok(normal!.title.includes('Auto') && pace, 'the route is in the tooltip and the pace line shows');
      ok(normal!.compareDocumentPosition(pace!) & Node.DOCUMENT_POSITION_FOLLOWING);
      ok(pace!.textContent!.toLowerCase().includes({ step: 'one step up', fast: 'being used fast', low: 'close to its limit' }[kind]));
      ok(host.querySelector<HTMLElement>('.model')!.title.includes('Effort: High'), 'effort is in the header tooltip');
      ok(!host.querySelector('.model')!.textContent!.includes('Effort'), 'and not in the header');
      equal(host.querySelectorAll('details.activity').length, 1);
      equal(host.querySelector<HTMLDetailsElement>('details.activity')!.open, false);
      equal(host.querySelector('.ai > .md')!.textContent!.trim(), 'Answer');
    } finally { dispose(); }
  }
});

check('pace: Settings separates own USD from shared quota and marks the daily line at fifty percent', async () => {
  const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
  useLanguage('en');
  const host = document.createElement('div');
  document.body.append(host);
  const until = Date.now() / 1000 + 3600;
  const usage = [{ vendor: 'claude', plan: 'max', state: 'ok', learning: false, relearning: false, outsideShare: 0.25,
    foreignUse: true, overageOn: false, billedPerUse: false, billingUnknown: false, askForPlan: false,
    paidPossible: false, ownSpendUsd: 1.25, unmetered: 0, counting: false,
    windows: [{ name: 'five_hour', used: 0.3, elapsed: 0.5, expected: 0.5, resetsAt: until }] }];
  const props = { start: 'accounts', look: { appearance: { base: 'sala' }, vars: {}, scheme: 'dark', motion: 'none', effects: {}, checks: [], notices: [] }, saved: {}, vendors: ['claude'],
    quotas: { 'claude:five_hour': { vendor: 'claude', window: 'five_hour', used: 0.3, resetsAt: until } },
    now: Date.now(), usage, onClose: () => {}, onSave: async () => true, onOpenSetup: () => {} } as unknown as Parameters<typeof Settings>[0];
  const dispose = render(() => createComponent(Settings, props), host);
  try {
    ok(host.textContent!.includes('30%'));
    ok(host.textContent!.includes('5-hour'));
    equal((host.querySelector('.expected') as HTMLElement | null)?.style.left, '50%');
    ok(host.textContent!.includes('$1.25') && !host.textContent!.includes('1.2500'), 'two decimals, never four');
    host.querySelector<HTMLButtonElement>('#acct-claude-details')!.click();
    await tick();
    ok(host.textContent!.includes('whole account'));
  } finally { dispose(); host.remove(); }
});

check('pace: paid credits require two clicks and name one vendor and one reply', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const old = api.approveCredits;
  const calls: unknown[][] = [];
  api.approveCredits = async (...args) => { calls.push(args); };
  const usage = [{ vendor: 'claude', billingUnknown: false, billedPerUse: false, paidPossible: true }] as Parameters<typeof QuotaCard>[0]['usage'];
  const dispose = render(() => createComponent(QuotaCard, { id: 'conversation', m: { id: 'reply', at: 1, role: 'assistant', text: '', vendor: 'claude', status: 'streaming', reason: 'reason.waiting' }, usage }), host);
  try {
    const button = (text: string) => [...host.querySelectorAll('button')].find((b) => b.textContent === text)!;
    button('Use paid credits for this message').click();
    equal(calls.length, 0, 'the offer alone spends nothing');
    ok(host.textContent!.includes(`your ${vendorName('claude')} account`), `confirmation must name its account: ${host.textContent}`);
    button('Send with paid credits').click();
    await Promise.resolve();
    equal(JSON.stringify(calls), JSON.stringify([['conversation', 'reply', 'claude']]));
  } finally { dispose(); host.remove(); api.approveCredits = old; }
});

check('pace: restoring a failed draft keeps attachments and refuses to overwrite newer input', () => {
  const host = document.createElement('div');
  document.body.append(host);
  let restore: ((draft: Draft) => boolean) | undefined;
  let clear: ((draft: Draft) => boolean) | undefined;
  const props = { vendors: ['claude'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onSend: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onRestore: (f: (draft: Draft) => boolean) => { restore = f; },
    onClearRestored: (f: (draft: Draft) => boolean) => { clear = f; } } as unknown as Parameters<typeof Composer>[0];
  const dispose = render(() => createComponent(Composer, props), host);
  try {
    const saved = { text: 'Kept text', images: [{ mediaType: 'image/png', base64: 'aA==' }], files: [{ name: 'data.pdf', base64: 'YQ==' }] };
    ok(restore, 'the composer supplies its restoration function');
    equal(restore!(saved), true);
    equal(host.querySelector('textarea')!.value, 'Kept text');
    ok(host.textContent!.includes('data.pdf'));
    equal(restore!({ text: 'Older text', images: [], files: [] }), false);
    equal(host.querySelector('textarea')!.value, 'Kept text');
    equal(clear!(saved), true, 'an accepted resume clears only the restored draft');
    equal(host.querySelector('textarea')!.value, '');
    equal(restore!(saved), true);
    const field = host.querySelector('textarea')!;
    field.value = 'Newer text'; field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    equal(clear!(saved), false, 'a user edit is not cleared by an older resume');
    equal(field.value, 'Newer text');
  } finally { dispose(); host.remove(); }
});

check('pace: a failed snapshot after an accepted send never restores a request already sent', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const before = { open: api.open, send: api.send, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const storage = localStorage.getItem('faber.place');
  const c: Conversation = { id: 'accepted-test', title: 'Test', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {}, messages: [] };
  const initial = { conversations: [{ id: c.id, title: c.title, area: c.area, updatedAt: 1 }], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test',
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } }
  } as AppState;
  let opens = 0, accepted = 0;
  api.open = async () => { if (++opens === 1) return c; throw new Error('post-send snapshot failed'); };
  api.send = async () => { accepted++; return c.id; };
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: c.id }));
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
  try {
    await new Promise((resolve) => setTimeout(resolve, 0));
    const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = 'Send exactly once'; field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    host.querySelector('form.composer')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    equal(accepted, 1);
    equal(field.value, '', 'snapshot failure cannot put a sent request back in the box');
    ok(host.textContent!.includes('post-send snapshot failed'));
  } finally {
    dispose(); host.remove(); Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});
