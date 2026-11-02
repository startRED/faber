import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppState, type Conversation, type Message } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { ContextMenuHost } from '../../ui/ContextMenu.tsx';
import { installContextMenu } from '../../ui/contextmenu.ts';
import { Composer, type Draft } from '../../ui/Composer.tsx';
import { QuotaCard } from '../../ui/QuotaCard.tsx';
import { t, useLanguage } from '../../ui/i18n.ts';
import { vendorName } from '../../ui/models.ts';
import { Settings } from '../../ui/Settings.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { check, equal, ok } from './check.ts';

check('move chat to project: App restores keyboard focus after its delayed refresh unless the person moved focus', async () => {
  installContextMenu();
  const before = { open: api.open, state: api.state, moveToProject: api.moveToProject, setClock: api.setClock, setShellTexts: api.setShellTexts, notesSeen: api.notesSeen };
  const storage = localStorage.getItem('faber.place');
  const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    for (const focusElsewhere of [false, true]) {
      const c: Conversation = { id: 'moving-open-chat', title: 'Trip notes', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {}, messages: [] };
      const moved: Conversation = { ...c, area: 'project', projectId: 'garden' };
      const initial = { conversations: [{ id: c.id, title: c.title, area: c.area, updatedAt: 1 }], projects: [{ id: 'garden', name: 'Garden', path: 'X:/garden' }], vendors: ['claude'],
        settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test',
        look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
          appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } }
      } as AppState;
      let moving = false;
      let finishRefresh: ((s: AppState) => void) | undefined;
      api.open = async () => moving ? moved : c;
      api.moveToProject = async () => { moving = true; return moved; };
      api.state = () => new Promise<AppState>((resolve) => { finishRefresh = resolve; });
      api.setClock = async () => {}; api.setShellTexts = async () => {}; api.notesSeen = async () => Number.MAX_SAFE_INTEGER;
      localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: c.id }));
      const host = document.createElement('div');
      // Keep the fixture in view: focus scrolling would dismiss the keyboard menu, especially in WebKit.
      host.style.cssText = 'position:fixed;inset:0';
      document.body.append(host);
      const elsewhere = document.createElement('input'); document.body.append(elsewhere);
      const dispose = render(() => [createComponent(App, { initial, inbox: { take: () => {} } }), createComponent(ContextMenuHost, {})], host);
      try {
        await tick();
        const row = host.querySelector<HTMLButtonElement>('.crow .item')!;
        ok(row, 'the open Chat row exists'); row.focus({ preventScroll: true });
        equal(document.activeElement, row, 'the Chat row has focus before its keyboard menu opens');
        row.dispatchEvent(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true, cancelable: true }));
        await tick();
        // The page's first ContextMenuHost draws the menu, including when it belongs to this test's App.
        const entry = (label: string) => {
          const items = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')];
          const item = items.find((b) => b.textContent === label);
          ok(item, `menu offers ${label}: ${items.map((b) => b.textContent).join(', ')}`);
          return item;
        };
        entry(t('menu.moveToProject')).click(); await tick();
        entry('Garden').click(); await tick();
        ok(finishRefresh, 'App requested an asynchronous refresh');
        ok(!host.querySelector('.crow'), 'the area changed before the row snapshot arrived');
        if (focusElsewhere) {
          elsewhere.focus({ preventScroll: true });
          equal(document.activeElement, elsewhere, 'the person moved focus to another control');
          // Hidden Chromium can change activeElement without emitting focusin.
          elsewhere.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
          elsewhere.blur();
        }
        finishRefresh({ ...initial, conversations: [{ ...initial.conversations[0]!, area: 'project', projectId: 'garden' }] });
        await tick();
        const destination = host.querySelector<HTMLButtonElement>('.proj-chats .crow .item');
        ok(destination, 'the refreshed project has the moved row');
        equal(document.activeElement, focusElsewhere ? document.body : destination,
          focusElsewhere ? 'the row must not reclaim focus after the person focused and blurred another control' : 'delayed focus follows the moved row');
      } finally { dispose(); host.remove(); elsewhere.remove(); }
    }
  } finally {
    Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

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
      ok(host.querySelector('.model .effort')!.textContent!.includes('high effort'), 'the effort shows beside the model');
      ok(!host.querySelector<HTMLElement>('.model')!.title.includes('ffort'), 'and not again in the tooltip');
      equal(host.querySelectorAll('.steps.one .step').length, 1, 'the single step stays visible');
      const words = [...host.querySelectorAll('.ai > .md')].map((part) => part.textContent!.trim());
      equal(words.join('|'), 'Checking the files.|Answer');
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

check('move chat to project: errored quota recovery offers Stop before the chat can move', async () => {
  const original = api.interrupt;
  const calls: string[] = [];
  try {
    for (const error of ['error.quotaOut', 'error.quotaWait']) {
      const [message, setMessage] = createSignal<Message>({ id: 'reply', role: 'assistant', text: '', at: 1, status: 'error', vendor: 'claude', error });
      api.interrupt = async (id) => { calls.push(id); setMessage((m) => ({ ...m, status: 'stopped', error: undefined })); };
      const host = document.createElement('div');
      // Solid's delegated click handler lives on document, so the card must be mounted there.
      document.body.append(host);
      const dispose = render(() => createComponent(QuotaCard, { id: 'conversation', get m() { return message(); }, usage: [] }), host);
      try {
        const stop = [...host.querySelectorAll('button')].find((b) => b.textContent === t('composer.stop'));
        ok(stop, `${error}: Stop is reachable even when the reply is an error`);
        stop.click();
        await Promise.resolve();
        equal(message().status, 'stopped');
        ok(![...host.querySelectorAll('button')].some((b) => b.textContent === t('composer.stop')), 'the stopped recovery has no live action');
      } finally { dispose(); host.remove(); }
    }
    equal(JSON.stringify(calls), JSON.stringify(['conversation', 'conversation']));
  } finally { api.interrupt = original; }
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

check('a reply that stopped on its own says why in one line: calm for a limit, red for a failure, never beside "Stopped."', () => {
  useLanguage('en');
  const user = { id: 'u', role: 'user', text: 'go', at: 1 } as Message;
  const kinds = (['reason.fail.limit', 'reason.fail.rateLimit', 'reason.fail.overloaded', 'reason.fail.connection', 'reason.fail.signIn', 'reason.fail.rejected', 'reason.fail.contextFull'] as const).map((k) => [k, false] as const);
  for (const [error, calm] of [['reason.turnLimit', true], ['reason.budget', true], ['reason.failed', false], ['reason.other', false], ...kinds] as const) {
    const m = { id: 'reply', role: 'assistant', text: 'Partial', at: 2, status: 'stopped', vendor: 'claude', error } as Message;
    const host = document.createElement('div');
    const dispose = render(() => createComponent(Thread, { conversationId: 'chat', messages: [user, m], from: 0, options: [], onBranch: () => {} }), host);
    try {
      const why = [...host.querySelectorAll('.note')].filter((n) => n.textContent === t(error));
      equal(why.length, 1, error + ' is said once');
      equal(why[0]!.classList.contains('err'), !calm, error + (calm ? ' is calm' : ' is red'));
      ok(!host.textContent!.includes(t('answer.stopped')), error + ': no second line saying Stopped');
    } finally { dispose(); }
  }
  // The person's own Stop leaves no reason and still says so.
  const stopped = { id: 'reply', role: 'assistant', text: 'Partial', at: 2, status: 'stopped', vendor: 'claude' } as Message;
  const host = document.createElement('div');
  const dispose = render(() => createComponent(Thread, { conversationId: 'chat', messages: [user, stopped], from: 0, options: [], onBranch: () => {} }), host);
  try { ok(host.textContent!.includes(t('answer.stopped'))); } finally { dispose(); }
});
