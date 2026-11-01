import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type AppState, type Choice, type Conversation, type Message, type Summary, type VendorUsage } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { Composer } from '../../ui/Composer.tsx';
import { closeMenu, menu } from '../../ui/contextmenu.ts';
import { amount, compact, lang, money, percent, t, useLanguage } from '../../ui/i18n.ts';
import { Running } from '../../ui/Running.tsx';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { keepRows } from '../../ui/sidebar.ts';
import { Thread } from '../../ui/Thread.tsx';
import { keep, kept, release } from '../../ui/unsent.ts';
import { agentKind } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// Delivery 1 of the calm screens (docs/superpowers/specs/2026-10-16-calm-screens-design.md): work screens with one place per
// thing, one line per idea, and a light screen while an answer streams.

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));

const surface = (width = 660) => {
  const root = document.createElement('div');
  root.style.cssText = `position:fixed;left:0;top:0;width:${width}px`;
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  return root;
};

const question: Message = { id: 'q', role: 'user', text: 'Fix the parser', at: 0 };
const answer = (extra: Partial<Message> = {}): Message => ({ id: 'a', role: 'assistant', text: 'Done.', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5', effort: 'high', ...extra });

function thread(messages: Message[], extra: Partial<Parameters<typeof Thread>[0]> = {}, width = 660) {
  const root = surface(width);
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'calm', options: [], onBranch: () => {}, ...extra }), root);
  return { root, close: () => { dispose(); root.remove(); } };
}

check('calm: under an answer, four icon buttons with names, a step up when offered, the time and the cost, all on one line', () => {
  useLanguage('en');
  const nudge = { reason: 'nudge.failedCheck', vendor: 'claude', model: 'claude-opus-5-5', effort: 'medium' } as const;
  const reply = answer({ costUsd: 0.0042, inputTokens: 22_000, outputTokens: 310, cacheWarmUntil: Date.now() + 3_600_000, nudge, took: 94_000 });
  for (const width of [660, 420]) {
    const view = thread([question, reply], { options: [{ label: 'Claude Sonnet', choice: { vendor: 'claude', model: 'sonnet' } }] }, width);
    try {
      const actions = view.root.querySelector<HTMLElement>('.actions')!;
      equal(view.root.querySelectorAll('select').length, 0, 'no model picker under an answer: the model is chosen in the composer only');
      const icons = [...actions.querySelectorAll<HTMLButtonElement>('button.iconbtn')];
      equal(icons.length, 4, 'copy, read aloud, retry, open in terminal');
      for (const button of icons) {
        ok(button.title && button.getAttribute('aria-label'), 'each icon button has a tooltip and an accessible name');
        ok(button.querySelector('svg') && button.textContent === '', 'and shows an icon, no words');
      }
      deepEqual(icons.map((b) => b.getAttribute('aria-label')), [t('answer.copy'), t('answer.listen'), t('answer.retry', { model: t('model.auto') }), t('answer.terminal')]);
      ok(actions.querySelector('.when time') && actions.querySelector('.when .took'), 'the time it was sent and how long it took');
      const cost = actions.querySelector<HTMLElement>('.cost')!;
      equal(cost.textContent, money(0.0042, 4), 'the visible cost is the number alone');
      ok(cost.title.includes(compact(22_000)) && cost.title.includes(t('answer.cost', { cost: money(0.0042, 4) })), `the tooltip holds the tokens: ${cost.title}`);
      ok(cost.title.includes(t('answer.cacheWarm', { time: '' }).split('{')[0]!.trim() || 'cache'), 'and how long the cache stays warm');
      ok(!actions.textContent!.includes('cache') && !actions.textContent!.includes(compact(22_000)), 'none of it is on the line');
      equal(actions.querySelectorAll('.nudge').length, 1);
      const middles = [...actions.children].map((el) => Math.round(el.getBoundingClientRect().top + el.getBoundingClientRect().height / 2));
      ok(Math.max(...middles) - Math.min(...middles) <= 3, `${width}px: nothing wraps onto a second line: ${middles}`);
      // The row's own box may be stretched by the grid of the article around it; what matters is that no control is taller than one.
      ok(Math.max(...[...actions.children].map((el) => el.getBoundingClientRect().height)) <= 36, `${width}px: the line is one control high`);
      ok(actions.scrollWidth <= actions.clientWidth + 1, `${width}px: nothing spills out of the line`);
    } finally {
      view.close();
    }
  }
});

check('calm: a retry says which model it goes to, the one the composer has, in its tooltip and in the right-click menu', () => {
  useLanguage('en');
  const calls: Array<[string, string, Choice | undefined]> = [];
  const choice: Choice = { vendor: 'claude', model: 'sonnet' };
  const view = thread([question, answer()], { options: [{ label: 'Claude Sonnet', choice }], choice, onBranch: (id, text, c) => { calls.push([id, text, c]); } });
  try {
    const retry = view.root.querySelector<HTMLButtonElement>('.actions button[aria-label^="Retry"]')!;
    equal(retry.title, t('answer.retry', { model: 'Claude Sonnet' }));
    retry.click();
    deepEqual(calls, [['q', 'Fix the parser', undefined]], 'the retry carries no model of its own: the screen uses the composer');
    view.root.querySelector('article')!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 10, clientY: 10 }));
    // The menu's items, not its drawing: whether a menu host is on the page depends on which checks ran before.
    ok(menu()?.items.some((item) => item.label === t('answer.retry', { model: 'Claude Sonnet' })), 'the menu item names the model too');
    closeMenu();
  } finally {
    view.close();
  }
});

check('calm: the answer header is the mark, the model and its effort; a wait for quota is said once, by the card', () => {
  useLanguage('en');
  const waiting = answer({ status: 'streaming', text: '', reason: 'reason.waiting', error: 'error.quotaWait', waitUntil: Date.now() / 1000 + 7200 });
  const view = thread([question, waiting]);
  try {
    const said = view.root.textContent!.match(/Waiting for included quota/g) ?? [];
    equal(said.length, 1, 'the card says it, the header and the red note do not');
    equal(view.root.querySelector('.model')!.textContent, 'Sonnet 5.5· high effort');
    equal(view.root.querySelector('.note.err'), null);
    ok(view.root.querySelector('.quota-card')!.textContent!.includes('Waiting for included quota'));
  } finally {
    view.close();
  }
});

check('calm: while an answer streams, its blocks keep their nodes, and a change to the tools builds no text at all', async () => {
  const [message, setMessage] = createSignal<Message>(answer({ status: 'streaming', text: 'First paragraph.\n\nSecond paragraph' }));
  const view = thread([]);
  view.close();
  const root = surface();
  const dispose = render(() => createComponent(Thread, { get messages() { return [question, message()]; }, from: 0, conversationId: 'calm-md', options: [], onBranch: () => {} }), root);
  try {
    const paragraphs = () => [...root.querySelectorAll<HTMLElement>('.ai > .md > div > p')];
    const [first, second] = paragraphs();
    equal(paragraphs().length, 2);
    setMessage({ ...message(), text: 'First paragraph.\n\nSecond paragraph grows' });
    equal(paragraphs()[0], first, 'the finished block keeps its node (a selection in it survives)');
    equal(paragraphs()[1], second, 'the open block keeps its node while it grows');
    equal(second!.textContent, 'Second paragraph grows');
    const open = paragraphs()[1];
    setMessage({ ...message(), tools: [{ id: 't1', name: 'Bash', input: { command: 'ls' }, at: message().text.length, ok: true }] });
    equal(paragraphs()[0], first);
    equal(paragraphs()[1], open, 'a tick that changed only the tools touches no text');
    setMessage({ ...message(), text: 'First paragraph.\n\nSecond paragraph grows\n\nThird' });
    equal(paragraphs()[0], first);
    equal(paragraphs()[1], open, 'a block that is closed is left alone once the next one opens');
    equal(paragraphs().length, 3);
  } finally {
    dispose();
    root.remove();
  }
});

check('calm: the answer is still sanitised block by block, raw HTML included, and reference links still resolve', () => {
  const bad = '<img src=x onerror="window.__calmXss = 1">\n\nafter <script>window.__calmXss = 2</script> text\n\n[docs][1]\n\n[1]: https://example.com/docs';
  for (const text of [bad, 'plain\n\n[docs][1] and <b onclick="window.__calmXss = 3">bold</b>\n\n[1]: https://example.com/docs']) {
    const view = thread([question, answer({ text })]);
    try {
      ok(!view.root.querySelector('[onerror], [onclick], script'), 'nothing that can run reaches the page');
      ok(view.root.querySelector('a[href="https://example.com/docs"]'), 'a reference link defined in another block is a link');
    } finally {
      view.close();
    }
  }
  equal((window as unknown as { __calmXss?: number }).__calmXss, undefined);
});

check('calm: a reference link whose definition streams in later becomes a link, and stays one when the answer ends', () => {
  const [message, setMessage] = createSignal<Message>(answer({ status: 'streaming', text: 'See [docs][1] here.' }));
  const root = surface();
  const dispose = render(() => createComponent(Thread, { get messages() { return [question, message()]; }, from: 0, conversationId: 'calm-links', options: [], onBranch: () => {} }), root);
  try {
    equal(root.querySelector('.ai .md a'), null, 'no definition yet, no link');
    setMessage({ ...message(), text: 'See [docs][1] here.\n\n[1]: https://example.com/docs' });
    ok(root.querySelector('.ai .md a[href="https://example.com/docs"]'), 'the earlier block is drawn again with the definition');
    setMessage({ ...message(), status: 'done', text: 'See [docs][1] here.\n\n[1]: https://example.com/docs\n\nMore.' });
    ok(root.querySelector('.ai .md a[href="https://example.com/docs"]'), 'a new block after it keeps the link');
  } finally {
    dispose();
    root.remove();
  }
});

check('calm: the unsent text is written on every change, so a crash right after a key loses nothing', () => {
  const key = 'calm-draft';
  const writes: string[] = [];
  const original = Storage.prototype.setItem;
  Storage.prototype.setItem = function (name: string, value: string) { if (name.endsWith(key)) writes.push(value); return original.call(this, name, value); };
  try {
    for (const text of ['a', 'ab', 'abc']) keep(key, text);
    deepEqual(writes, ['a', 'ab', 'abc'], 'each change is in storage before the next key');
    equal(localStorage.getItem(`faber.unsent.${key}`), 'abc', 'what a restart reads is what was typed last');
    keep(key, '');
    equal(kept(key), '', 'an emptied box forgets at once');
  } finally {
    Storage.prototype.setItem = original;
    release(key);
    localStorage.removeItem(`faber.unsent.${key}`);
  }
});

check('calm: the composer measures its height at most once per frame, however many keys arrive', async () => {
  const root = surface(500);
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onSend: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
  }), root);
  try {
    await settle();
    const field = root.querySelector<HTMLTextAreaElement>('textarea')!;
    let measured = 0;
    const rect = field.getBoundingClientRect.bind(field);
    field.getBoundingClientRect = () => (measured++, rect());
    for (let i = 1; i <= 30; i++) {
      field.value = 'line\n'.repeat(i);
      field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    }
    await settle(80);
    ok(measured >= 1 && measured <= 3, `thirty keys in one frame measured ${measured} times`);
  } finally {
    dispose();
    root.remove();
  }
});

const usageOf = (state: VendorUsage['state'], counting = false): VendorUsage[] => [{
  vendor: 'codex', plan: 'pro', state, learning: false, relearning: false, outsideShare: null, foreignUse: false, overageOn: false, billedPerUse: false,
  billingUnknown: false, askForPlan: false, paidPossible: false, ownSpendUsd: 0, unmetered: 0, counting,
  windows: [{ name: 'seven_day', used: 0.44, elapsed: 0.5, expected: 0.5, resetsAt: 1_800_086_400, readAt: 1_800_000_000 - 240, uncounted: null }],
}];

function sidebar(area: 'chat' | 'project', over: Partial<Parameters<typeof Sidebar>[0]> = {}, width = 224) {
  const root = surface(width);
  const conversations: Summary[] = [
    { id: 'c1', title: 'A chat about sky', area: 'chat', updatedAt: 1_800_000_000_000 },
    { id: 'c2', title: 'Another chat', area: 'chat', updatedAt: 1_800_000_000_000 },
    { id: 'p1', title: 'Project conversation', area: 'project', projectId: 'demo', updatedAt: 1_800_000_000_000 },
  ];
  const dispose = render(() => createComponent(Sidebar, {
    conversations, projects: [{ id: 'demo', name: 'Demo', path: '/demo' }], vendors: ['codex'], version: '9.8.7', place: { area, projectId: 'demo' }, running: [],
    quotas: { 'codex:seven_day': { vendor: 'codex', window: 'seven_day', used: 0.44, resetsAt: 1_800_086_400 } }, usage: usageOf('ok'), now: 1_800_000_000_000,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {}, onOpen: () => {}, onRemove: () => {}, onSettings: () => {}, ...over,
  }), root);
  return { root, close: () => { dispose(); root.remove(); } };
}

check('calm: the sidebar lists only the open area: chats in Chat, projects with their conversations in Projects', () => {
  useLanguage('en');
  const inChat = sidebar('chat', { projects: [] });
  try {
    equal(inChat.root.querySelectorAll('.list .crow').length, 2, 'the chats');
    equal(inChat.root.querySelector('.projects, .prow, .side-empty'), null, 'no Projects block, not even an empty one');
    ok(!inChat.root.textContent!.includes(t('side.noProjects')) && !inChat.root.textContent!.includes('Project conversation'));
  } finally {
    inChat.close();
  }
  const inProjects = sidebar('project');
  try {
    equal(inProjects.root.querySelectorAll('.prow').length, 1);
    equal(inProjects.root.querySelectorAll('.proj-chats .crow').length, 1, "the project's conversation");
    equal(inProjects.root.querySelector('.list'), null, 'no chat is listed');
    ok(!inProjects.root.textContent!.includes('A chat about sky'));
  } finally {
    inProjects.close();
  }
});

check('calm: the sidebar footer is one row with the name and Settings, then one thin line per AI; the version waits in a tooltip', () => {
  useLanguage('en');
  const view = sidebar('chat', { userName: 'Erick' });
  try {
    equal(view.root.querySelector('.version'), null, 'no version line');
    const row = view.root.querySelector<HTMLElement>('.side-foot .foot-row')!;
    equal(row.querySelector('.who')!.textContent, 'Erick');
    const settings = row.querySelector<HTMLButtonElement>('button')!;
    ok(settings.textContent!.includes(t('settings.open')) && settings.title.includes('v9.8.7'), `Settings carries the version: ${settings.title}`);
    const middles = [...row.children].map((el) => Math.round(el.getBoundingClientRect().top + el.getBoundingClientRect().height / 2));
    ok(Math.max(...middles) - Math.min(...middles) <= 3, `the row is one line: ${middles}`);
    const line = view.root.querySelector<HTMLElement>('.usage .vendor')!;
    ok(line.querySelector('.mk') && line.querySelector('.meter') && line.querySelector('.share'), 'its mark, a thin bar and the share');
    equal(line.querySelector('.share')!.textContent, percent(0.44));
    equal(line.querySelector('.used')!.textContent, t('quota.used'), 'and says it is used');
    equal(line.querySelector('.age, .extra'), null, 'no age or estimate on the line');
    ok(line.title.includes(t('quota.read', { when: '' }).split('{')[0]!.trim() || 'Updated'), `the age is in the tooltip: ${line.title}`);
    ok(!line.classList.contains('low'), 'amber only near the limit');
    equal(line.querySelector<HTMLElement>('.meter')!.getBoundingClientRect().height <= 4, true, 'thin');
  } finally {
    view.close();
  }
  const nameless = sidebar('chat');
  try {
    equal(nameless.root.querySelector('.who'), null, 'without a name the row holds Settings alone');
  } finally {
    nameless.close();
  }
});

check('calm: the usage line never wraps, "still counting" reads "≥ share" with the words in the tooltip, and amber appears only near the limit', () => {
  useLanguage('pt-BR');
  const view = sidebar('chat', { usage: usageOf('low', true) });
  try {
    const line = view.root.querySelector<HTMLElement>('.usage .vendor')!;
    ok(line.classList.contains('low'), 'the engine says it is nearly used');
    ok(line.querySelector('.share')!.textContent!.startsWith('≥ ') && line.title.includes(t('quota.counting')), `"≥" and the words: ${line.title}`);
    ok(!line.textContent!.includes('…'), 'nothing that reads as cut text');
    ok(line.getBoundingClientRect().height <= 32, `one line even in Portuguese: ${line.getBoundingClientRect().height}`);
    ok(line.querySelector('.name')!.textContent === 'Codex', 'the vendor name shows on the line');
  } finally {
    view.close();
    useLanguage('en');
  }
});

check('calm: a refreshed list keeps the sidebar rows that did not change', async () => {
  useLanguage('en');
  const rows: Summary[] = [
    { id: 'c1', title: 'One', area: 'chat', updatedAt: 1_800_000_000_000 },
    { id: 'c2', title: 'Two', area: 'chat', updatedAt: 1_800_000_000_000 },
  ];
  const fresh = (list: Summary[]) => list.map((r) => ({ ...r }));
  const next = keepRows(rows, fresh(rows));
  ok(next[0] === rows[0] && next[1] === rows[1], 'equal rows are the ones already held');
  ok(keepRows(rows, fresh(rows)) === rows || keepRows(rows, fresh(rows)).every((r, i) => r === rows[i]));
  const renamed = keepRows(rows, [{ ...rows[0]!, title: 'One!' }, { ...rows[1]! }]);
  ok(renamed[0] !== rows[0] && renamed[1] === rows[1], 'only the changed row is new');
  const [held, setHeld] = createSignal(rows);
  const root = surface();
  const dispose = render(() => createComponent(Sidebar, {
    get conversations() { return held(); }, projects: [], vendors: [], version: '1', place: { area: 'chat' }, running: [], quotas: {}, now: 1_800_000_000_000,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {}, onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
  }), root);
  try {
    const before = [...root.querySelectorAll<HTMLElement>('.list .crow')];
    equal(before.length, 2);
    setHeld((h) => keepRows(h, fresh(h)));
    await settle();
    ok([...root.querySelectorAll('.list .crow')].every((row, i) => row === before[i]), 'the rows kept their nodes');
    setHeld((h) => keepRows(h, [{ ...h[0]!, title: 'One!' }, { ...h[1]! }]));
    await settle();
    const after = [...root.querySelectorAll<HTMLElement>('.list .crow')];
    ok(after.includes(before[1]!), 'the row that did not change is the very node it was');
    const renamed = after.find((row) => row.textContent!.includes('One!'));
    ok(renamed && renamed !== before[0], 'the renamed one is built again');
  } finally {
    dispose();
    root.remove();
  }
});

check('calm: the running column says "nothing running" once and has no footer sentence', () => {
  useLanguage('en');
  const root = surface(360);
  const dispose = render(() => createComponent(Running, { items: [], projects: [], onOpen: () => {}, onClose: () => {}, modal: false, ref: () => {} }), root);
  try {
    equal(root.querySelector('.rc-foot'), null);
    equal(root.querySelector('.rc-empty p')!.textContent, t('run.none'));
    equal((root.textContent!.match(new RegExp(t('run.none'), 'g')) ?? []).length, 1);
  } finally {
    dispose();
    root.remove();
  }
});

check('calm: helpers read as words, and counts of zero are plural in every language', () => {
  useLanguage('en');
  equal(agentKind('general-purpose'), t('agent.kind.general'));
  equal(agentKind('Explore'), 'Explore');
  equal(agentKind('code-reviewer'), 'Code reviewer');
  equal(agentKind('Codex'), 'Codex');
  equal(agentKind(''), '');
  const before = lang();
  try {
    for (const [code, zero, one] of [['en', '0 steps', '1 step'], ['pt-BR', '0 etapas', '1 etapa'], ['es', '0 pasos', '1 paso'], ['fr', '0 étapes', '1 étape'], ['de', '0 Schritte', '1 Schritt']] as const) {
      useLanguage(code);
      equal(t('activity.steps', { n: 0 }), zero, code);
      equal(t('activity.steps', { n: 1 }), one, code);
    }
  } finally {
    useLanguage(before);
  }
});

check('calm: numbers are written the way each language writes them', () => {
  const before = lang();
  try {
    useLanguage('en');
    equal(compact(22_000), '22K');
    equal(percent(0.72), '72%');
    equal(money(0.0189, 4), '$0.0189');
    useLanguage('pt-BR');
    ok(compact(22_000).includes('mil'), compact(22_000));
    equal(amount(0.0189, 4), '0,0189');
    ok(money(0.5, 2).startsWith('US$'), money(0.5, 2));
  } finally {
    useLanguage(before);
  }
});

check('calm: a helper sent to the background is a ring and a running one a dot, never the square of a stop button; the row lines up with the work log', () => {
  useLanguage('en');
  const agent = { id: 'a1', name: 'Agent', input: { description: 'Audit the docs', subagent_type: 'general-purpose', run_in_background: true } };
  const view = thread([question, answer({ status: 'streaming', text: '', tools: [{ id: 'b1', name: 'Bash', input: { command: 'ls' }, ok: true }, agent] })]);
  try {
    const row = view.root.querySelector<HTMLElement>('.agent')!;
    equal(row.dataset.state, 'background');
    const mark = row.querySelector<HTMLElement>('.agent-mark .live')!;
    const style = getComputedStyle(mark);
    equal(style.borderRadius, '50%', 'round, not square');
    equal(style.backgroundColor, 'rgba(0, 0, 0, 0)', 'hollow while it works in the background');
    equal(row.querySelector('.agent-meta')!.textContent, [t('agent.kind.general'), t('agent.background'), t('activity.steps', { n: 0 })].join(' · '));
    const step = view.root.querySelector<HTMLElement>('.step-line .step-icon')!;
    ok(step.getBoundingClientRect().left >= view.root.getBoundingClientRect().left, 'the step stays inside the conversation');
    ok(row.getBoundingClientRect().left >= view.root.getBoundingClientRect().left, 'the helper stays inside the conversation');
  } finally {
    view.close();
  }
});

// The shell around the work, mounted whole.
async function shell(vendors: AppState['vendors'], conversation: Conversation, run: (root: HTMLElement) => Promise<void> | void) {
  const originalOpen = api.open;
  const originalClock = api.setClock;
  const stored = localStorage.getItem('faber.place');
  const effects = { ambient: false, texture: false, glow: false };
  const initial: AppState = {
    vendors, models: [], quotas: [], projects: [], version: '0.0.0', running: { revision: 0, items: [] },
    conversations: [{ id: conversation.id, title: conversation.title, area: conversation.area, updatedAt: 1 }],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: {
      vars: themes.css.base, scheme: 'dark', motion: 'none', effects, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects },
    },
  };
  api.open = async () => conversation;
  api.setClock = async () => {};
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: conversation.id }));
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
  try {
    await settle(60);
    await run(root);
  } finally {
    dispose();
    root.remove();
    api.open = originalOpen;
    api.setClock = originalClock;
    if (stored === null) localStorage.removeItem('faber.place');
    else localStorage.setItem('faber.place', stored);
  }
}

const chat = (): Conversation => ({ id: 'calm-chat', title: 'Calm chat', area: 'chat', sessions: {}, createdAt: 0, updatedAt: 1, messages: [question, answer()] });

check('calm: a conversation opens with no permission sentence on top and no model chip in the top bar', async () => {
  useLanguage('en');
  await shell(['claude'], chat(), (root) => {
    ok(root.querySelector('article.ai'), 'the conversation is open');
    equal(root.querySelector('.col .hint, [role=status].hint'), null, 'no permission strip');
    equal(root.querySelector('.topbar .chip'), null, 'no read-only model chip');
    ok(root.querySelector('.composer .chip.choose'), 'the model is chosen in the composer');
    equal(root.querySelector('.composer .effort-hint'), null);
    ok(!/works without asking|asks before running/i.test(root.querySelector('.col')!.textContent!), 'no permission words in the conversation');
  });
});

check('calm: with no AI connected there is no composer to type in', async () => {
  useLanguage('en');
  await shell([], chat(), (root) => {
    equal(root.querySelector('.composer'), null);
    equal(root.querySelector('.dock'), null);
  });
});

check('calm: the usage bar changes by a clip that eases, never by its width or a scale, and stands still with reduced motion', () => {
  useLanguage('en');
  const view = sidebar('chat');
  try {
    view.root.dataset.motion = 'full';
    const bar = view.root.querySelector<HTMLElement>('.usage .vendor .meter i')!;
    equal(bar.style.width, '', 'the share is not written as a width');
    equal(bar.style.clipPath.replace(/0px/g, '0'), 'inset(0 56% 0 0)', 'it is the same 44 % as the number beside it, cut from the right');
    equal(getComputedStyle(bar).transform, 'none', 'never a scale, which would squeeze a striped fill');
    equal(bar.getBoundingClientRect().width, bar.parentElement!.getBoundingClientRect().width, 'the bar keeps its real width');
    equal(getComputedStyle(bar).transitionProperty, 'clip-path', 'only the cut eases');
    view.root.dataset.motion = 'reduced';
    equal(getComputedStyle(bar).transitionDuration, '0s', 'reduced motion jumps');
  } finally {
    view.close();
  }
});
