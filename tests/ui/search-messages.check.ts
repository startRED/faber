import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState, type Area, type Conversation, type Message, type MessageHit, type MessageSearch, type Summary } from '../../ui/api.ts';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
// Waits for a state instead of a fixed time, so a slow runner cannot fail the check.
const until = async (done: () => unknown, limit = 4000) => {
  for (let waited = 0; waited < limit && !done(); waited += 20) await settle(20);
};
const hit = (area: Area): MessageHit => ({
  conversationId: `search-${area}`, area, title: 'A saved conversation', messageIndex: 3,
  snippet: 'Found <b>café</b> in an answer.', highlights: [{ start: 9, end: 13 }],
});
const type = (root: HTMLElement, text: string) => {
  const field = root.querySelector<HTMLInputElement>('.search input')!;
  field.value = text;
  field.dispatchEvent(new Event('input', { bubbles: true }));
};

check('search-messages: the sidebar debounces, drops stale replies, marks safe snippets and searches only its area', async () => {
  const original = api.searchMessages;
  const calls: [string, Area][] = [];
  let old!: (result: MessageSearch) => void;
  api.searchMessages = async (query, area) => {
    calls.push([query, area]);
    if (query === 'old') return new Promise((resolve) => { old = resolve; });
    if (query === 'failed') throw new Error('failed');
    return { ready: true, hits: query === 'missing' || query === 'saved' ? [] : [hit(area), hit(area === 'chat' ? 'project' : 'chat')] };
  };
  const [area, setArea] = createSignal<Area>('chat');
  const [conversation, setConversation] = createSignal<string>();
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const dispose = render(() => createComponent(Sidebar, {
    conversations: ['chat', 'project'].map((area) => ({ id: `search-${area}`, area: area as Area, title: 'A saved conversation', updatedAt: 0 })),
    projects: [], vendors: [], version: 'test', get place() { return { area: area(), id: conversation() }; },
    running: [], quotas: {}, now: 0, onNew: () => {}, onArea: () => {}, onProject: () => {},
    onNewIn: () => {}, onAddProject: () => {}, onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
  }), root);
  try {
    type(root, 'o');
    type(root, 'old');
    equal(calls.length, 0, 'keystrokes do not each start a command');
    ok(root.textContent?.includes(t('side.searching')));
    await settle(220);
    equal(calls.length, 1);
    type(root, 'café');
    await settle(220);
    equal(root.querySelectorAll('.message-hit').length, 1, 'another area never leaks into the results');
    equal(root.querySelector('mark')!.textContent, 'café');
    equal(root.querySelector('.hit-snippet b'), null, 'a snippet is text, never injected markup');
    ok(root.querySelector('.hit-snippet')!.textContent?.includes('<b>café</b>'));
    old({ ready: true, hits: [] });
    await settle();
    equal(root.querySelectorAll('.message-hit').length, 1, 'the earlier request cannot replace the newest results');
    const button = root.querySelector('.message-hit');
    const searched = calls.length;
    setConversation('search-chat');
    equal(root.querySelector('.message-hit'), button, 'opening a conversation in the same area keeps its clickable hit');
    await settle(220);
    equal(calls.length, searched, 'opening a conversation in the same area does not repeat the search');
    setArea('project');
    await settle(220);
    equal(calls.at(-1)![1], 'project');
    equal(root.querySelectorAll('.message-hit').length, 1);
    setArea('chat');
    type(root, 'saved');
    await settle(220);
    equal(root.querySelectorAll('.crow').length, 1, 'a matching title stays above the message results');
    ok(root.querySelector('.message-hits')!.textContent?.includes(t('side.noMatch')), 'no matching messages is said even when a title matches');
    type(root, 'missing');
    await settle(220);
    ok(root.textContent?.includes(t('side.noMatch')));
    type(root, 'failed');
    await settle(220);
    ok(root.textContent?.includes(t('side.searchFailed')));
    equal(root.querySelector('[aria-busy]')!.getAttribute('aria-busy'), 'false');
    type(root, '');
    equal(root.querySelector('.message-hits'), null, 'clearing search releases its result DOM');
  } finally {
    dispose(); root.remove(); api.searchMessages = original;
  }
});

check('search-messages: refreshing conversations keeps a focused hit mounted and updates it by conversation and message', async () => {
  const original = api.searchMessages;
  try {
    for (const area of ['chat', 'project'] as const) {
      let refresh!: (result: MessageSearch) => void;
      let calls = 0;
      api.searchMessages = async () => ++calls === 1 ? { ready: true, hits: [hit(area)] } : new Promise((resolve) => { refresh = resolve; });
      const summary: Summary = { id: `search-${area}`, title: 'A saved conversation', area, updatedAt: 0 };
      const [conversations, setConversations] = createSignal([summary]);
      let opened: MessageHit | undefined;
      const root = document.createElement('div');
      root.dataset.motion = 'none';
      document.body.append(root);
      const dispose = render(() => createComponent(Sidebar, {
        get conversations() { return conversations(); }, projects: [], vendors: [], version: 'test', place: { area },
        running: [], quotas: {}, now: 0, onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {},
        onAddProject: () => {}, onOpen: () => {}, onMessage: (hit) => { opened = hit; }, onRemove: () => {}, onSettings: () => {},
      }), root);
      try {
        type(root, 'café');
        await settle(220);
        const button = root.querySelector<HTMLButtonElement>('.message-hit')!;
        button.focus();
        equal(document.activeElement, button);
        setConversations([{ ...summary, updatedAt: 1 }, { id: 'other', area, title: 'Another conversation', updatedAt: 1 }]);
        equal(root.querySelector('.message-hit'), button, 'a list refresh leaves the old hit in place during the debounce');
        equal(document.activeElement, button, 'a list refresh never removes the keyboard target');
        await settle(220);
        equal(root.querySelector('.message-hit'), button, 'the hit remains mounted while the refreshed search is pending');
        refresh({ ready: true, hits: [{ ...hit(area), title: 'Updated title', snippet: 'Updated café match', highlights: [{ start: 8, end: 12 }] }] });
        await settle();
        equal(root.querySelector('.message-hit'), button, 'fresh objects with the same conversation and message reuse the button');
        equal(document.activeElement, button, 'the refreshed result keeps keyboard focus');
        ok(button.textContent?.includes('Updated title'));
        button.click();
        equal(opened?.snippet, 'Updated café match', 'the retained button opens its fresh result');
        setConversations([{ ...summary, updatedAt: 2 }]);
        await settle(220);
        refresh({ ready: true, hits: [{ ...hit(area), messageIndex: 5 }] });
        await settle();
        ok(root.querySelector('.message-hit') !== button, 'a different message gets its own button');
      } finally { dispose(); root.remove(); }
    }
  } finally { api.searchMessages = original; }
});

check('search-messages: message-only matches show the same archived label as title matches in both areas', async () => {
  const original = api.searchMessages;
  try {
    for (const area of ['chat', 'project'] as const) {
      api.searchMessages = async () => ({ ready: true, hits: [hit(area)] });
      const root = document.createElement('div');
      document.body.append(root);
      const dispose = render(() => createComponent(Sidebar, {
        conversations: [{ id: `search-${area}`, title: 'A saved conversation', area, projectId: area === 'project' ? 'project' : undefined, updatedAt: 0, archived: true }],
        projects: [{ id: 'project', name: 'Project', path: 'E:\\fixture' }], vendors: [], version: 'test', place: { area }, running: [], quotas: {}, now: 0,
        onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
        onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
      }), root);
      try {
        type(root, 'café');
        await settle(220);
        equal(root.querySelectorAll('.crow').length, 0, 'the archived conversation matches only in its message');
        equal(root.querySelector('.message-hit .tag-quiet')?.textContent, t('archive.tag'));
        type(root, 'saved');
        await settle(220);
        const titleTag = getComputedStyle(root.querySelector('.item .tag-quiet')!);
        const messageTag = getComputedStyle(root.querySelector('.message-hit .tag-quiet')!);
        equal(messageTag.fontSize, titleTag.fontSize, 'message hits reuse the archived label size');
        equal(messageTag.color, titleTag.color, 'message hits reuse the archived label ink');
      } finally { dispose(); root.remove(); }
    }
  } finally { api.searchMessages = original; }
});

check('search-messages: clicking a hit opens its old message, unfolds its run and respects every motion level in both areas', async () => {
  const original = { open: api.open, send: api.send, clock: api.setClock, search: api.searchMessages, notes: api.notesSeen, scroll: Element.prototype.scrollIntoView };
  const stored = localStorage.getItem('faber.place');
  const effects = { ambient: false, texture: false, glow: false };
  let scrolled: ScrollIntoViewOptions | undefined;
  Element.prototype.scrollIntoView = function (options) {
    scrolled = typeof options === 'object' ? options : undefined;
    original.scroll.call(this, options);
  };
  api.setClock = async () => {};
  api.notesSeen = async () => Number.MAX_SAFE_INTEGER;
  try {
    for (const area of ['chat', 'project'] as const) for (const motion of ['full', 'reduced', 'none'] as const) {
      const conversation: Conversation = {
        id: `search-${area}`, title: 'A saved conversation', area, projectId: area === 'project' ? 'project' : undefined,
        sessions: {}, createdAt: 0, updatedAt: 0,
        messages: Array.from({ length: 100 }, (_, i) => ({
          id: `message-${i}`, at: i, role: i % 2 ? 'assistant' : 'user', status: 'done',
          text: i === 3 ? 'The middle of the answer mentions café.' : 'A message with enough words to fill the reading column. '.repeat(10),
          ...(i === 2 || i === 4 ? { wake: { why: 'wake.keepGoing' } } : {}),
        })),
      };
      api.open = async (id) => { equal(id, conversation.id); return conversation; };
      let receive: ((event: AppEvent) => void) | undefined;
      api.send = async (draft) => {
        const question: Message = { id: 'new-question', at: 100, role: 'user', text: draft.text };
        const answer: Message = { id: 'new-answer', at: 101, role: 'assistant', text: 'The newest answer is visible.', status: 'done' };
        receive?.({ type: 'message', conversationId: conversation.id, message: question });
        receive?.({ type: 'message', conversationId: conversation.id, message: answer });
        conversation.messages = [...conversation.messages, question, answer];
        return conversation.id;
      };
      api.searchMessages = async (_query, searchedArea) => { equal(searchedArea, area); return { ready: true, hits: [hit(area)] }; };
      const initial: AppState = {
        vendors: ['codex'], models: [], quotas: [], version: 'test', running: { revision: 0, items: [] },
        projects: area === 'project' ? [{ id: 'project', name: 'Project', path: 'E:\\fixture' }] : [],
        conversations: [{ id: conversation.id, title: conversation.title, area, projectId: conversation.projectId, updatedAt: 0 }],
        settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
        look: { vars: themes.css.base, scheme: 'dark', motion, effects, checks: [], notices: [],
          appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion, effects } },
      };
      localStorage.setItem('faber.place', JSON.stringify({ area, projectId: conversation.projectId }));
      const root = document.createElement('div');
      root.style.cssText = 'position:fixed;inset:0';
      root.dataset.motion = motion;
      for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
      document.body.append(root);
      const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener) => { receive = listener; } } }), root);
      try {
        type(root, 'café');
        await until(() => root.querySelector('.message-hit'));
        const button = root.querySelector<HTMLButtonElement>('.message-hit');
        ok(button, `${area}: the existing search returns the message hit`);
        scrolled = undefined;
        button.click();
        await until(() => root.querySelector('[data-message-index="3"]') === document.activeElement);
        const target = root.querySelector<HTMLElement>('[data-message-index="3"]');
        const mounted = [...root.querySelectorAll<HTMLElement>('[data-message-index]')].map((el) => el.dataset.messageIndex);
        ok(target, `${area}, ${motion}: an old message outside the latest page is mounted (mounted: ${mounted.join(',') || 'none'}; screen: ${root.querySelector('.welcome, .thread')?.className ?? 'neither'}; error: ${root.querySelector('[role=alert]')?.textContent ?? 'none'})`);
        ok(target.textContent?.includes('mentions café'));
        ok(target.closest<HTMLDetailsElement>('.run-block')?.open, 'a hit folded into a run becomes visible');
        ok(target.classList.contains('search-focus'), 'the message gets a static outline');
        equal(document.activeElement, target, 'keyboard and screen reader focus follows the opened message');
        const scroll = scrolled as ScrollIntoViewOptions | undefined;
        equal(scroll?.block, 'center');
        const full = motion === 'full' && !matchMedia('(prefers-reduced-motion: reduce)').matches;
        equal(scroll?.behavior, full ? 'smooth' : 'instant');
        const run = target.closest<HTMLDetailsElement>('.run-block')!;
        run.open = false;
        await settle();
        receive?.({ type: 'message', conversationId: conversation.id, message: { ...conversation.messages[5]!, text: 'Another streaming chunk', status: 'streaming' } });
        await settle();
        equal(run.open, false, 'a streaming update preserves a run the person collapsed after the search jump');
        button.click();
        await until(() => root.querySelector('[data-message-index="3"]')?.closest<HTMLDetailsElement>('.run-block')?.open);
        ok(root.querySelector('[data-message-index="3"]')?.closest<HTMLDetailsElement>('.run-block')?.open, 'a new navigation to the same hit opens its run once again');
        ok(root.querySelectorAll('[data-message-index]').length <= 50, 'jumping back does not mount the whole history');
        const thread = root.querySelector<HTMLElement>('.thread')!;
        thread.scrollTop = thread.scrollHeight;
        thread.dispatchEvent(new Event('scroll'));
        ok(root.querySelector('[data-message-index="70"]'), 'scrolling onward loads the next page');
        button.click();
        await settle(60);
        const textarea = root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
        textarea.value = 'Continue from here';
        textarea.dispatchEvent(new Event('input', { bubbles: true }));
        root.querySelector<HTMLButtonElement>('.composer button[type="submit"]')!.click();
        await until(() => root.querySelector('[data-message-index="101"]'));
        ok(root.querySelector('[data-message-index="101"]')?.textContent?.includes('The newest answer'), 'sending leaves the search page and shows the new answer');
        equal(root.querySelector('.search-focus'), null, 'sending clears the old search marker');
        equal(root.querySelector('[data-message-index="3"]'), null, 'sending returns to the latest page without retaining the old history');
        api.searchMessages = async () => ({ ready: true, hits: [{ ...hit(area), messageIndex: 100, snippet: 'Continue from here', highlights: [{ start: 0, end: 8 }] }] });
        type(root, 'continue');
        await until(() => root.querySelector('.message-hit')?.textContent?.includes('Continue'));
        root.querySelector<HTMLButtonElement>('.message-hit')!.click();
        await until(() => root.querySelector('[data-message-index="100"].search-focus'));
        ok(root.querySelector('[data-message-index="100"].search-focus'), 'a hit in the person’s own words opens its message too');
        root.querySelector<HTMLButtonElement>('[data-message-index="100"] .edit')!.click();
        const edit = root.querySelector<HTMLTextAreaElement>('.you.editing textarea')!;
        edit.value = 'An unfinished edit';
        edit.dispatchEvent(new Event('input', { bubbles: true }));
        root.querySelector<HTMLButtonElement>('.message-hit')!.click();
        await until(() => root.querySelector('.you.editing.search-focus'));
        ok(root.querySelector('.you.editing.search-focus'), 'an open edit keeps its search anchor');
        equal(root.querySelector<HTMLTextAreaElement>('.you.editing textarea')!.value, 'An unfinished edit', 'searching never loses an unsent edit');
        root.querySelector<HTMLButtonElement>('.you.editing .actions button[type="button"]')!.click();
        root.querySelector<HTMLButtonElement>('.newchat')!.click();
        await settle();
        equal(root.querySelector('[data-message-index]'), null, 'a new conversation retains no old message window');

        conversation.messages = conversation.messages.slice(0, 6);
        api.searchMessages = async () => ({ ready: true, hits: [hit(area)] });
        type(root, 'café');
        await until(() => root.querySelector('.message-hit'));
        root.querySelector<HTMLButtonElement>('.message-hit')!.click();
        // The reveal is consumed on a later frame, which a hidden window may hold back.
        await until(() => root.querySelector('[data-message-index="3"]')?.closest<HTMLDetailsElement>('.run-block')?.open);
        const revealed = root.querySelector('[data-message-index="3"]')?.closest<HTMLDetailsElement>('.run-block');
        ok(revealed?.open, 'search reveals the earlier answer in the active run');
        const activeThread = root.querySelector<HTMLElement>('.thread')!;
        activeThread.scrollTop = activeThread.scrollHeight;
        activeThread.dispatchEvent(new Event('scroll'));
        revealed.open = false;
        await settle();
        const automatic: Message[] = [
          { id: 'automatic-question', at: 6, role: 'user', text: 'Continue', wake: { why: 'wake.keepGoing' } },
          { id: 'automatic-answer', at: 7, role: 'assistant', text: 'The next automatic turn', status: 'streaming' },
        ];
        for (const message of automatic) receive?.({ type: 'message', conversationId: conversation.id, message });
        conversation.messages = [...conversation.messages, ...automatic];
        await settle();
        const regrouped = root.querySelector<HTMLDetailsElement>('.run-block');
        ok(regrouped && regrouped !== revealed && !revealed.isConnected, 'the next automatic turn mounts a new run block');
        equal(root.querySelectorAll('.run-block').length, 1, 'the mounted run includes all automatic replies');
        equal(regrouped.open, false, 'the mounted run stays folded after its search navigation was consumed');
        root.querySelector<HTMLButtonElement>('.message-hit')!.click();
        await until(() => root.querySelector('[data-message-index="3"]')?.closest<HTMLDetailsElement>('.run-block')?.open);
        ok(root.querySelector('[data-message-index="3"]')?.closest<HTMLDetailsElement>('.run-block')?.open, 'a fresh search navigation reveals the regrouped answer again');
      } finally { dispose(); root.remove(); }
    }
  } finally {
    api.open = original.open; api.send = original.send; api.setClock = original.clock; api.searchMessages = original.search; api.notesSeen = original.notes;
    Element.prototype.scrollIntoView = original.scroll;
    if (stored === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', stored);
  }
});
