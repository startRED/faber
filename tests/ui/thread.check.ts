import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import { App } from '../../ui/App.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { api, type AppState, type Conversation, type Message, type RewindRequest } from '../../ui/api.ts';
import { LANGUAGES, t, templates, useLanguage } from '../../ui/i18n.ts';
import { renderMarkdown } from '../../ui/markdown.tsx';
import { followTail, readingPoint, restoreReading } from '../../ui/reading.ts';
import { helpersAtWork } from '../../ui/worklog.ts';
import { check, deepEqual, equal, frames, ok } from './check.ts';
import { closeMenu, menu } from '../../ui/contextmenu.ts';

check('thread: 300 messages open at the newest 40, prepend without a jump and load message 5 for rewind', async () => {
  const original = { ...api };
  const storage = new Map(['faber.place', 'faber.reading'].map((key) => [key, localStorage.getItem(key)]));
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0;z-index:100';
  document.body.append(root);
  let dispose = () => {};
  const id = 'paged-300';
  let messages: Message[] = Array.from({ length: 300 }, (_, i) => ({
    id: `paged-${i}`, role: i % 2 ? 'assistant' : 'user', at: i + 1,
    text: `Message ${i + 1}. ` + 'Words that stay at the same reading position. '.repeat(10),
    ...(i % 2 ? { status: 'done' as const,
      tools: [{ id: `paged-tool-${i}`, name: 'Read', input: { file_path: `file-${i}.txt` }, ok: true, output: 'Saved output.' }] } : {}),
  }));
  const page = (): Conversation => ({ id, title: 'Paged conversation', area: 'chat', sessions: {}, createdAt: 1, updatedAt: 1,
    earlier: Math.max(0, messages.length - 40), messages: messages.slice(-40) });
  const requests: string[] = [];
  let redoReads = 0;
  const restored: RewindRequest[] = [];
  let release: (() => void) | undefined;
  const waitFor = async (ready: () => boolean, why: string) => {
    for (let i = 0; i < 100 && !ready(); i++) await new Promise((done) => setTimeout(done, 20));
    ok(ready(), why);
    await frames();
  };
  try {
    const base = await original.state();
    const initial: AppState = { ...base, conversations: [{ id, title: 'Paged conversation', area: 'chat', updatedAt: 1 }],
      groups: [], projects: [], vendors: ['claude'], running: { revision: 0, items: [] }, background: { revision: 0, items: [] }, quotas: [], usage: [],
      look: { ...base.look, motion: 'none' },
      settings: { ...base.settings, appearance: { ...base.look.appearance, motion: 'none' }, language: 'en', setup: 'done', tutorialDone: true } };
    Object.assign(api, {
      state: async () => initial, open: async () => page(), versions: async () => [], visualAnswers: async () => [],
      rewindRedos: async () => { redoReads++; return []; }, rewindPoints: async () => [], commands: async () => [],
      warm: async () => {}, seen: async () => {}, notesSeen: async () => Number.MAX_SAFE_INTEGER,
      updateStatus: async () => ({ phase: 'none' }), onUpdate: async () => () => {},
    });
    api.messagesBefore = async (conversationId, first) => {
      equal(conversationId, id);
      requests.push(first);
      if (requests.length === 1) await new Promise<void>((done) => { release = done; });
      const at = messages.findIndex((m) => m.id === first);
      return messages.slice(Math.max(0, at - 40), at);
    };
    api.searchMessages = async () => ({ ready: true, hits: [{ conversationId: id, area: 'chat', title: 'Paged conversation',
      messageIndex: 4, snippet: 'Fifth message needle', highlights: [] }] });
    api.rewindPreview = async (request) => ({ files: [], replacements: 0, recreations: 0, removals: 0, conflicts: 0,
      messages: messages.length - 4, coverage: { saved: 0, skipped: 0, unavailable: true, incomplete: false },
      running: [], recovery: false, redoExpiresAt: null, notice: request.messageId });
    api.rewindRestore = async (request) => {
      restored.push(request);
      messages = messages.slice(0, 4);
      return { id: 'restored', restored: [], kept: [], failed: [], partial: false, recovery: false, notice: null };
    };
    localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id }));
    localStorage.removeItem('faber.reading');
    dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
    await waitFor(() => !!root.querySelector('[data-message-index="299"]'), 'the newest message opens');
    const thread = root.querySelector<HTMLDivElement>('.thread')!;
    equal(thread.querySelectorAll('[data-message-index]').length, 40, 'only the newest 40 are drawn');
    equal(thread.querySelector('[data-message-index]')?.getAttribute('data-message-index'), '260', 'indices retain the unloaded offset');
    ok(thread.scrollHeight - thread.scrollTop - thread.clientHeight < 3, 'opening starts at the actual tail');
    equal(requests.length, 0, 'opening never requests the old history');
    const redosBeforePaging = redoReads;
    thread.scrollTop = 1;
    thread.dispatchEvent(new Event('scroll'));
    thread.dispatchEvent(new Event('scroll'));
    await waitFor(() => !!release, 'scrolling to the first drawn message requests one page');
    equal(requests.length, 1, 'overlapping scrolls share one request');
    const mine = thread.querySelector('[data-message-index="260"]')!;
    const answer = thread.querySelector('[data-message-index="261"]')!;
    const step = answer.querySelector('.steps.one')!;
    const paragraph = answer.querySelector('.md p')!;
    const words = paragraph.firstChild!;
    const selection = window.getSelection()!;
    const range = document.createRange();
    range.setStart(words, 0); range.setEnd(words, 7);
    selection.removeAllRanges(); selection.addRange(range);
    const before = thread.querySelector<HTMLElement>('[data-message-index="260"]')!.getBoundingClientRect().top;
    const computedStyle = window.getComputedStyle;
    let rowStyles = 0;
    window.getComputedStyle = (element, pseudo) => {
      if (element.parentElement?.parentElement === thread && element.hasAttribute('data-message-index')) rowStyles++;
      return computedStyle.call(window, element, pseudo);
    };
    try {
      release!();
      await waitFor(() => !!thread.querySelector('[data-message-index="220"]'), 'the previous 40 are prepended');
    } finally { window.getComputedStyle = computedStyle; }
    equal(rowStyles, 0, 'paging does not inspect every message style for list animation');
    equal(thread.querySelectorAll('[data-message-index]').length, 80, 'loaded messages are retained');
    equal(redoReads, redosBeforePaging, 'loading earlier messages never rereads rewind history');
    const after = thread.querySelector<HTMLElement>('[data-message-index="260"]')!.getBoundingClientRect().top;
    ok(Math.abs(after - before) < 3, `prepending keeps the reading position: ${before} -> ${after}`);
    equal(thread.querySelector('[data-message-index="260"]'), mine, 'prepending retains the user message node');
    equal(thread.querySelector('[data-message-index="261"]'), answer, 'prepending retains the answer node');
    equal(answer.querySelector('.md p'), paragraph, 'prepending does not parse an existing answer again');
    equal(answer.querySelector('.steps.one'), step, 'prepending retains the existing tool group');
    equal(thread.querySelectorAll('.ai .steps.one').length, 40, 'a complete history page keeps every mounted tool group');
    equal(paragraph.firstChild, words, 'prepending retains the selected text node');
    equal(selection.toString(), 'Message', 'prepending preserves the selected words');
    const hitTest = document.elementsFromPoint;
    let lookups = 0;
    document.elementsFromPoint = (x, y) => { lookups++; return hitTest.call(document, x, y); };
    try { thread.dispatchEvent(new Event('scroll')); }
    finally { document.elementsFromPoint = hitTest; }
    equal(lookups, 0, 'the scroll from a restored viewport never rereads the known anchor');
    selection.removeAllRanges();
    const search = root.querySelector<HTMLInputElement>('.search input')!;
    search.value = 'needle'; search.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await waitFor(() => !!root.querySelector('.message-hits button'), 'search provides a link to an unloaded message');
    root.querySelector<HTMLButtonElement>('.message-hits button')!.click();
    await waitFor(() => !!thread.querySelector('[data-message-index="4"].search-focus'), 'the jump loads all pages needed for message 5');
    const fifth = thread.querySelector<HTMLElement>('[data-message-index="4"]')!;
    ok(fifth.textContent?.includes('Message 5.'), 'the global index names the correct message');
    fifth.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 100, clientY: 100 }));
    const rewind = menu()?.items.find((item) => item.id === 'rewind');
    ok(rewind, 'the loaded old message offers rewind');
    closeMenu();
    rewind!.run();
    await waitFor(() => !!document.querySelector('.rewind-confirm .primary:not(:disabled)'), 'the rewind preview is ready');
    const conversation = document.querySelector<HTMLInputElement>('.rewind-confirm input[value="conversation"]')!;
    conversation.click();
    await frames();
    document.querySelector<HTMLButtonElement>('.rewind-confirm .primary')!.click();
    await waitFor(() => restored.length === 1 && !!thread.querySelector('[data-message-index="3"]') && !thread.querySelector('[data-message-index="4"]'), 'rewind refreshes the shortened conversation');
    equal(restored[0]!.messageId, 'paged-4', 'rewind reaches message 5 rather than page position 5');
  } finally {
    release?.(); closeMenu(); dispose(); root.remove(); Object.assign(api, original);
    for (const [key, value] of storage) { if (value === null) localStorage.removeItem(key); else localStorage.setItem(key, value); }
  }
});

check('thread: restoring a known reading anchor measures only that row and still skips hidden or removed messages', () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;z-index:9999;top:50px;left:300px;width:300px;height:100px;overflow:auto';
  document.body.append(root);
  const messages: Message[] = Array.from({ length: 80 }, (_, i) => ({ id: `anchor-${i}`, role: 'user', text: 'Words', at: 0 }));
  const measured: number[] = [];
  const rows = messages.map((message, i) => {
    const row = document.createElement('div');
    row.dataset.messageIndex = String(i); row.style.height = '20px'; row.textContent = message.text;
    const rectangles = row.getClientRects;
    row.getClientRects = () => { measured.push(i); return rectangles.call(row); };
    root.append(row);
    return row;
  });
  const space = document.createElement('div'); space.style.height = '100px'; root.append(space);
  try {
    const point = { id: 'anchor-50', index: 50, offset: 0 };
    restoreReading(root, point, messages);
    deepEqual(measured, [50], 'restoring one known row never measures all eighty messages');
    ok(Math.abs(rows[50]!.getBoundingClientRect().top - root.getBoundingClientRect().top) < 1, 'the known row keeps its offset');
    const inspected = new Set<number>();
    for (const [i, row] of rows.entries()) {
      const rectangle = row.getBoundingClientRect;
      row.getBoundingClientRect = () => { inspected.add(i); return rectangle.call(row); };
    }
    equal(readingPoint(root, messages)?.id, 'anchor-50', 'hit testing keeps the first visible logical message');
    ok(inspected.size < 8, `finding the visible anchor never measures all earlier rows: ${inspected.size}`);
    const position = root.scrollTop;
    let writes = 0;
    Object.defineProperty(root, 'scrollTop', { configurable: true, get: () => position, set: () => { writes++; } });
    try {
      restoreReading(root, point, messages);
      equal(writes, 0, 'an unchanged reading anchor never sends a redundant scroll to the compositor');
    } finally { Reflect.deleteProperty(root, 'scrollTop'); }
    rows[50]!.style.display = 'none';
    restoreReading(root, point, messages);
    ok(Math.abs(rows[51]!.getBoundingClientRect().top - root.getBoundingClientRect().top) < 1, 'a hidden anchor uses its visible neighbour');
    restoreReading(root, { ...point, id: 'removed', index: 500 }, messages);
    ok(Math.abs(rows[79]!.getBoundingClientRect().top - root.getBoundingClientRect().top) < 1, 'a missing anchor beyond the end uses the last visible row');
  } finally { root.remove(); }
});

check('thread: cached message timestamps keep native date and time words in every language', () => {
  const root = document.createElement('div'); document.body.append(root);
  const [at, setAt] = createSignal(1);
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [{ id: 'stamp', role: 'user' as const, text: 'Words', at: at() }]; },
    from: 0, conversationId: 'stamp', options: [], onBranch: () => {},
  }), root);
  try {
    for (const { code } of LANGUAGES) {
      useLanguage(code);
      for (const value of [1, 1700000123456, 1741503600000]) {
        setAt(value);
        const date = new Date(value);
        equal(root.querySelector('time')!.textContent, date.toLocaleTimeString(code, { hour: 'numeric', minute: '2-digit' }));
        equal(root.querySelector('.when')!.getAttribute('title'), date.toLocaleString(code, { dateStyle: 'full', timeStyle: 'medium' }));
        equal(root.querySelector('time')!.getAttribute('datetime'), date.toISOString());
      }
    }
    setAt(0);
    equal(root.querySelector('time'), null, 'an absent timestamp remains hidden');
  } finally { dispose(); root.remove(); useLanguage('en'); }
});

check('thread: a dead helper leaves the live count and keeps an uncertain cost while its caller continues', () => {
  for (const vendor of ['claude', 'codex'] as const) {
    for (const partial of ['', 'Keep these partial words.']) {
      const root = document.createElement('div');
      document.body.append(root);
      const call = { id: 'helper', name: 'Helper', input: { title: 'Check', brief: 'Check the files.' } };
      const [message, setMessage] = createSignal<Message>({ id: 'helper-exit', role: 'assistant', at: 0,
        status: 'streaming', vendor, text: 'Waiting for the helper.', tools: [call], costUsd: 0.01 });
      const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0,
        conversationId: 'helper-exit', options: [], onBranch: () => {} }), root);
      try {
        equal(helpersAtWork([message()]), 1);
        equal(root.querySelector<HTMLElement>('.team-member')?.dataset.state, 'running');
        setMessage({ ...message(), incomplete: true, tools: [{ ...call, ok: false, ended: 2,
          output: `${partial}\nThe helper process ended before reporting its result.` }] });
        equal(helpersAtWork([message()]), 0, 'the caller can continue without a ghost helper');
        equal(root.querySelector<HTMLElement>('.team-member')?.dataset.state, 'failed');
        ok(!root.querySelector('.team-member .st-working'), 'an exited helper has no live mark');
        equal(message().status, 'streaming', 'the caller remains free to continue its own reply');
        setMessage({ ...message(), status: 'done', text: 'The caller continued after the helper failed.' });
        ok(root.querySelector('.cost')?.textContent?.startsWith('≈'), 'known caller spend remains visibly uncertain');
        equal(message().tools?.[0]?.output?.includes(partial), true, 'partial words remain in the report');
      } finally { dispose(); root.remove(); }
    }
  }
});

check('thread: streaming preserves the open paragraph, selection, code scroll and safe link updates', () => {
  const root = document.createElement('div'); root.className = 'md'; document.body.append(root);
  try {
    renderMarkdown(root, 'A readable answer');
    const paragraph = root.firstChild!;
    const words = paragraph.firstChild!;
    const selection = window.getSelection()!;
    const range = document.createRange(); range.setStart(words, 2); range.setEnd(words, 10);
    selection.removeAllRanges(); selection.addRange(range);
    renderMarkdown(root, 'A readable answer grows naturally.');
    equal(root.firstChild, paragraph, 'the live paragraph does not disappear between deltas');
    equal(paragraph.firstChild, words, 'appending words preserves their text node');
    equal(selection.toString(), 'readable', 'new words do not clear the selected words');
    renderMarkdown(root, 'A readable answer grows naturally.\n\n```text\n' + 'wide '.repeat(100));
    const box = root.querySelector<HTMLElement>('.codeblock')!;
    const pre = box.querySelector('pre')!; pre.style.width = '120px'; pre.style.overflow = 'auto';
    pre.scrollLeft = 80;
    const copy = box.querySelector('button')!; copy.focus();
    renderMarkdown(root, 'A readable answer grows naturally.\n\n```text\n' + 'wide '.repeat(101));
    equal(root.querySelector('.codeblock'), box, 'the streaming code block is retained');
    equal(pre.scrollLeft, 80, 'the reader keeps their horizontal position');
    equal(document.activeElement, copy, 'streaming does not discard keyboard focus');
    renderMarkdown(root, 'Read [docs][d]\n\n[d]: https://example.com/one');
    const link = root.querySelector('a')!;
    renderMarkdown(root, 'Read [docs][d]\n\n[d]: https://example.com/two');
    equal(root.querySelector('a'), link);
    equal(link.getAttribute('href'), 'https://example.com/two');
    renderMarkdown(root, 'Read [docs][d]\n\n[d]: javascript:alert(1)');
    equal(root.querySelector('a')?.getAttribute('href'), null, 'removed unsafe attributes never survive reconciliation');
    renderMarkdown(root, '<span class="cb-copy">First answer</span>');
    renderMarkdown(root, '<span class="cb-copy">Corrected answer</span>');
    equal(root.textContent?.trim(), 'Corrected answer', 'answer HTML cannot impersonate a generated copy button');
    renderMarkdown(root, 'Short.');
    equal(root.textContent?.trim(), 'Short.', 'shorter replies release obsolete blocks');
  } finally { window.getSelection()?.removeAllRanges(); root.remove(); }
});

check('thread: ten tool steps fold while all words stay visible', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const narration = 'Checking the files.\n\nRunning the checks.\n\n';
  const final = 'The requested change is complete.\n\nAll checks passed.';
  const [message, setMessage] = createSignal<Message>({
    id: 'activity-check', role: 'assistant', at: 0, status: 'streaming', text: narration + final,
    textBreaks: ['Checking the files.\n\n'.length, narration.length], activityEnd: narration.length, thinking: 'Consider the simplest change.',
    tools: Array.from({ length: 10 }, (_, i) => ({ id: String(i), name: 'Bash', input: { command: `check ${i}` }, ok: i < 9 ? true : undefined })),
  });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'activity-check', options: [], onBranch: () => {},
  }), root);
  try {
    const head = root.querySelector<HTMLButtonElement>('.steps-head')!;
    equal(head.getAttribute('aria-expanded'), 'false');
    ok(head.textContent!.includes('check 9'), 'the running command remains visible');
    const words = () => [...root.querySelectorAll('.ai > .md')].map((part) => part.textContent).join('');
    ok(words().includes('Checking the files') && words().includes('The requested change is complete.') && words().includes('All checks passed.'), 'all words remain visible');
    setMessage({ ...message(), status: 'done' });
    equal(head.getAttribute('aria-expanded'), 'false');
    head.click();
    equal(root.querySelectorAll('.steps-list .step').length, 10, 'all ten commands are inspectable');
    ok(root.querySelector('.thought'), 'thinking stays visible in its own row');
    setMessage({ ...message(), textBreaks: undefined, activityEnd: undefined });
    ok(words().includes('Checking the files'), 'older replies keep their words');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: with Auto the model name shows as soon as it is chosen, while the answer streams, and stays the same at the end; its effort shows beside it', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'auto-name', role: 'assistant', at: 0, status: 'streaming', text: '', reason: 'reason.routing' });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'auto-name', options: [], onBranch: () => {},
  }), root);
  const header = () => root.querySelector<HTMLElement>('.model')!;
  const name = () => header().querySelector('b')!.textContent;
  try {
    equal(name(), 'Auto');
    // Erick, 2026-11-12: the choice shows happening in the answer, never as a blank wait.
    const live = () => root.querySelector<HTMLElement>('.faber-act.choosing');
    equal(live()?.textContent, `${t('act.faber')} · ${t('act.choosing')}`, 'while Auto chooses, a live row says so at once');
    equal(live()!.getAttribute('role'), 'status', 'and screen readers hear it');
    equal(header().querySelector('em'), null, 'the header does not say it a second time');
    equal(header().children.length, 1, 'the header is the mark with its name');
    const choosing = header().getBoundingClientRect().height;
    setMessage({
      ...message(), vendor: 'claude', model: 'claude-haiku-5-5', effort: 'medium',
      acts: [{ kind: 'route', actor: { who: 'faber' }, what: 'act.route.chose', params: { model: 'claude-haiku-5-5', effort: 'medium' } }],
    });
    equal(name(), 'Haiku 5.5', 'the name shows before the first word');
    equal(live(), null, 'once chosen the live row is gone');
    ok(root.querySelector('.faber-act')!.textContent!.includes('Haiku 5.5'), 'and the pick takes its place');
    equal(header().querySelector('em'), null, 'after choosing the reason is the tooltip');
    equal(header().textContent, `Haiku 5.5· ${t('answer.effort.medium')}`, 'the header is the mark, the model and its effort');
    ok(header().title.includes(t('reason.routing')), `the reason is in the tooltip: ${header().title}`);
    equal(header().getBoundingClientRect().height, choosing, 'and the header keeps its height');
    setMessage({ ...message(), text: 'Sunlight cont' });
    equal(name(), 'Haiku 5.5', 'and stays while the words arrive');
    setMessage({ ...message(), status: 'done' });
    equal(name(), 'Haiku 5.5', 'and is the same when the answer ends');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: a model the CLI never named reads the same while streaming and at the end, and never as "Codex Codex"', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'codex-name', role: 'assistant', at: 0, status: 'streaming', text: 'The', vendor: 'codex' });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'codex-name', options: [], onBranch: () => {},
  }), root);
  const name = () => root.querySelector<HTMLElement>('.model b')!.textContent;
  try {
    equal(name(), 'Codex', 'a model the CLI never named reads as its vendor');
    setMessage({ ...message(), model: 'codex' });
    equal(name(), 'Codex', "the engine's stand-in id is no name");
    setMessage({ ...message(), status: 'done' });
    equal(name(), 'Codex', 'and the end changes nothing');
    setMessage({ ...message(), model: 'gpt-6-luna' });
    equal(name(), 'GPT-6 Luna');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: the last answer and its actions leave at least 48px above either composer height', async () => {
  for (const [width, height] of [[1440, 900], [1000, 700]]) {
    const root = document.createElement('div');
    root.style.cssText = `position:fixed;left:0;top:0;width:${width}px;height:${height}px`;
    root.dataset.motion = 'none';
    for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
    root.innerHTML = '<div class="work" style="height:100%"><div class="thread"><div class="col"></div></div><div class="dock"></div></div>';
    document.body.append(root);
    const messages: Message[] = Array.from({ length: 30 }, (_, i) => ({
      id: String(i), role: i % 2 ? 'assistant' : 'user', text: 'A long thread with enough text to scroll. '.repeat(10),
      at: 0, status: 'done', vendor: 'codex', model: 'gpt-6',
    }));
    let fill!: (text: string) => void;
    const disposeThread = render(() => createComponent(Thread, {
      messages, from: 0, conversationId: 'gap-check', options: [], onBranch: () => {},
    }), root.querySelector('.col')!);
    const disposeComposer = render(() => createComponent(Composer, {
      vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
      onChoice: () => {}, onSend: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
      onReady: (ready) => { fill = ready; },
    }), root.querySelector('.dock')!);
    try {
      for (const text of ['', 'A very long prompt. '.repeat(500)]) {
        fill(text);
        await new Promise((resolve) => setTimeout(resolve, 40));
        const thread = root.querySelector<HTMLElement>('.thread')!;
        thread.scrollTop = thread.scrollHeight;
        ok(thread.scrollTop > 0, 'the thread must actually overflow');
        const last = root.querySelector('.col')!.lastElementChild!;
        ok(last.querySelector('.actions'), 'the measured answer includes its action row');
        const gap = root.querySelector('.composer')!.getBoundingClientRect().top - last.getBoundingClientRect().bottom;
        ok(gap >= 48 && gap <= 72, `${width}x${height}: gap ${gap}px with ${text ? 'expanded' : 'empty'} composer`);
      }
    } finally {
      disposeThread();
      disposeComposer();
      root.remove();
    }
  }
});

check('thread: a Codex work log reads cleanly: titles on their own lines, tidy command rows, the status sentence once', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const status = 'The window control did not connect.';
  const exe = '"C:\\\\WINDOWS\\\\System32\\\\WindowsPowerShell\\\\v1.0\\\\powershell.exe"';
  const inner = "Set-Content -LiteralPath 'a.txt' -Value 'one' -NoNewline";
  const [message, setMessage] = createSignal<Message>({
    id: 'worklog', role: 'assistant', at: 0, status: 'streaming', text: `${status}\n\n${status}\n\nI checked the files instead.`,
    textBreaks: [status.length + 2], activityEnd: status.length + 2,
    thinking: '**Minimizing output verbosity****Assessing time budget and test approach****Preparing for final retry**',
    tools: [{ id: 'c1', name: 'commandExecution', input: `${exe} -Command "${inner}"`, at: 0, ok: true }],
  });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'worklog', options: [], onBranch: () => {},
  }), root);
  try {
    root.querySelector<HTMLButtonElement>('.steps-head')!.click();
    root.querySelector<HTMLButtonElement>('.thought .step-line')!.click();
    const shown = () => root.textContent ?? '';
    ok(!shown().includes('**') && !shown().includes('commandExecution'));
    const titles = [...root.querySelectorAll('.thought-text p')].map((p) => p.textContent);
    equal(titles.join('|'), 'Minimizing output verbosity|Assessing time budget and test approach|Preparing for final retry');
    const row = root.querySelector<HTMLElement>('.step:not(.thought) .step-line')!;
    equal(row.querySelector('b')!.textContent, 'PowerShell');
    equal(row.querySelector('i')!.textContent, inner);
    equal(row.title, inner);
    equal(shown().split(status).length - 1, 2);
    setMessage({ ...message(), status: 'done' });
    equal(shown().split(status).length - 1, 2, 'completion preserves the words');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: running steps stay visible alone and in a fold', () => {
  const root = document.createElement('div'); document.body.append(root);
  const running = (id: string, command: string) => ({ id, name: 'Bash', input: { command }, ok: undefined });
  const [message, setMessage] = createSignal<Message>({ id: 'once', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [running('a', 'cargo build')] });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'once', options: [], onBranch: () => {} }), root);
  try {
    ok(root.querySelector('.steps.one .step[data-state="running"] .step-icon .st-working svg'));
    ok(root.querySelector('.step-line')!.textContent!.includes('cargo build'));
    setMessage({ ...message(), tools: [{ ...running('a', 'cargo build'), ok: true }, running('b', 'cargo test')] });
    const head = root.querySelector<HTMLButtonElement>('.steps-head')!;
    ok(head.textContent!.includes('cargo test'));
    head.click();
    equal(root.querySelectorAll('.steps-list .step').length, 2);
    ok(root.querySelector('.steps-list .step[data-state="running"]')!.textContent!.includes('cargo test'));
    ok(root.querySelector('.steps-list .step[data-state="running"] .step-icon .st-working svg'));
    equal(root.querySelector('.activity-latest'), null);
  } finally { dispose(); root.remove(); }
});

check('thread: the final answer is never cut, a long command wraps inside a 720 px column, and a command that looks like markup stays text', () => {
  const root = document.createElement('div');
  root.style.width = '720px';
  document.body.append(root);
  const long = `START ${'x'.repeat(200)} END`;
  const markup = '<img src=x onerror=alert(1)> **not bold** & <b>raw</b>';
  const narration = 'Check\n\n';
  const answer = 'Checking files.';
  const [message] = createSignal<Message>({
    id: 'worklog-edges', role: 'assistant', at: 0, status: 'done', text: narration + answer, textBreaks: [narration.length], activityEnd: narration.length,
    tools: [
      { id: 'l', name: 'Bash', input: { command: long }, ok: true },
      { id: 'm', name: 'Bash', input: { command: markup }, ok: true },
    ],
  });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'worklog-edges', options: [], onBranch: () => {},
  }), root);
  try {
    const words = [...root.querySelectorAll('.ai > .md')].map((part) => part.textContent).join('');
    ok(words.includes(narration.trim()) && words.includes(answer), 'all words stay visible');
    root.querySelector<HTMLButtonElement>('.steps-head')!.click();
    const [first, second] = [...root.querySelectorAll<HTMLElement>('.steps-list .step-line')];
    ok(first!.querySelector('i')!.textContent!.includes('START') && first!.querySelector('i')!.textContent!.includes('END'));
    first!.click();
    equal(first!.parentElement!.querySelector('.step-what')!.textContent, long, 'the full command is available');
    ok(first!.getBoundingClientRect().right <= root.getBoundingClientRect().right + 0.5, 'fits the column');
    equal(first!.title, long);
    equal(second!.querySelector('i')!.textContent, markup);
    equal(second!.querySelectorAll('img, b i, i b').length, 0);
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: words remain visible in order between steps while streaming and after completion', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const said = ['Sigo no código.', 'Corrigindo a parte 2.', 'Parte 2 pronta.', 'Atualizando o mapa do produto.'];
  const before = said.join('\n\n') + '\n\n';
  const tools = [{ id: '1', name: 'Bash', input: { command: 'one' }, ok: true, at: before.length }];
  const reply = (text: string, status: Message['status'] = 'streaming'): Message => ({ id: 'streamed', role: 'assistant', at: 0, status, text, tools });
  const [message, setMessage] = createSignal<Message>(reply(before + 'All done.'));
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'streamed', options: [], onBranch: () => {},
  }), root);
  try {
    const answer = root.querySelector('.ai')!;
    for (const status of ['streaming', 'done'] as const) {
      setMessage(reply(before + 'All done.', status));
      equal(root.querySelector('.ai'), answer, 'a new object for the same message keeps its answer node');
      const parts = [...root.querySelectorAll('.ai > .md, .ai > .steps')];
      deepEqual(parts.map((part) => part.classList.contains('md') ? 'words' : 'steps'), ['words', 'steps', 'words']);
      const words = parts.map((part) => part.textContent ?? '').join(' ');
      const phrases = [...said, 'All done.'];
      for (let i = 0; i < phrases.length; i++) {
        ok(words.includes(phrases[i]!), phrases[i]);
        if (i) ok(words.indexOf(phrases[i - 1]!) < words.indexOf(phrases[i]!), 'the words keep their order');
      }
    }
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: a held-back command says what it would have done, and Got it on the latest reply takes it off the needs-you list', async () => {
  const original = api.parkedSeen;
  const calls: [string, string][] = [];
  api.parkedSeen = async (id, turn) => void calls.push([id, turn]);
  const root = document.createElement('div');
  document.body.append(root);
  const held = (id: string, over: Partial<Message> = {}): Message => ({
    id, role: 'assistant', at: 1, status: 'done', text: 'Done.',
    parked: [{ toolUseId: `t-${id}`, tool: 'Bash', kind: 'command', label: '', does: 'delete', at: 1 }], ...over,
  });
  const [messages, setMessages] = createSignal<Message[]>([held('older'), held('latest')]);
  const dispose = render(() => createComponent(Thread, {
    get messages() { return messages(); }, from: 0, conversationId: 'held', options: [], onBranch: () => {},
  }), root);
  try {
    const cards = () => [...root.querySelectorAll('.ask.parked')];
    equal(cards().length, 2);
    ok(cards()[0]!.textContent?.includes(t('parked.does.delete')), 'the kind of command, never its words');
    equal(cards()[0]!.querySelector('button'), null, 'an older reply is only a record');
    const got = cards()[1]!.querySelector('button')!;
    equal(got.textContent, t('parked.seen'));
    got.click();
    await Promise.resolve();
    deepEqual(calls, [['held', 'latest']]);
    setMessages([held('older'), held('latest', { parkedSeen: true })]);
    equal(cards()[1]!.querySelector('button'), null, 'once read, the note stays without the button');
    setMessages([held('latest', { parked: [{ toolUseId: 't', tool: 'Bash', kind: 'command', label: '', at: 1 }] })]);
    ok(cards()[0]!.textContent?.includes(t('parked.command')), 'a command with no known kind keeps the plain words');
  } finally {
    dispose(); root.remove(); api.parkedSeen = original;
  }
});

check('thread: a reply Faber was closed under says so, offers Continue on the newest one, and the person’s own Stop stays "Stopped."', async () => {
  const user = { id: 'u', role: 'user', text: 'go', at: 1 } as Message;
  const reply = (closed: boolean) => ({ id: 'reply', role: 'assistant', text: 'Partial', at: 2, status: 'stopped', vendor: 'claude', closed }) as Message;
  const before = api.continueClosed;
  const asked: string[][] = [];
  api.continueClosed = async (id, turn) => { asked.push([id, turn]); };
  try {
    for (const area of ['chat', 'project'] as const) {
      // Attached, so the click reaches Solid's delegated handler.
      const host = document.createElement('div');
      document.body.append(host);
      const dispose = render(() => createComponent(Thread, { conversationId: 'c', area, messages: [user, reply(true)], from: 0, options: [], onBranch: () => {} }), host);
      try {
        ok(host.textContent!.includes(t('answer.closed')), area + ': the close is said');
        ok(!host.textContent!.includes(t('answer.stopped')), area + ': in place of "Stopped."');
        equal(host.textContent!.includes(t('answer.closedFiles')), area === 'project', area + ': the files line only in a project');
        const button = [...host.querySelectorAll('button')].find((b) => b.textContent === t('answer.closedContinue'));
        ok(button, area + ': the newest such reply offers Continue');
        button!.click();
      } finally { dispose(); host.remove(); }
    }
    deepEqual(asked, [['c', 'reply'], ['c', 'reply']], 'Continue names the conversation and the cut reply');
    // An older cut reply says it, without the button; the person's own Stop says "Stopped." as before.
    const later = [{ id: 'u2', role: 'user', text: 'more', at: 3 } as Message, { ...reply(false), id: 'reply2', at: 4 }];
    const host = document.createElement('div');
    const dispose = render(() => createComponent(Thread, { conversationId: 'c', messages: [user, reply(true), ...later], from: 0, options: [], onBranch: () => {} }), host);
    try {
      ok(host.textContent!.includes(t('answer.closed')));
      ok(![...host.querySelectorAll('button')].some((b) => b.textContent === t('answer.closedContinue')), 'no Continue once the reply is not the newest');
      ok(host.textContent!.includes(t('answer.stopped')), 'the person’s Stop is still said');
    } finally { dispose(); }
  } finally { api.continueClosed = before; }
});

check('thread: the answer area speaks plain words, never git, cache or CLI words', () => {
  // The engineering words each language used for these texts before (the locale audit of 2026-10-29).
  // Whole words only: "programa" holds "rama" and "cliquez" holds "cli".
  const words = (list: string) => new RegExp(`(^|[^\\p{L}])(${list})($|[^\\p{L}])`, 'iu');
  const jargon: Record<string, RegExp> = {
    en: words('branch|cache|CLI|session|assistant'),
    'pt-BR': words('galho|cache|CLI|sessão|assistente'),
    es: words('rama|caché|CLI|sesión|asistente'),
    fr: words('branche|cache|CLI|session|assistant'),
    de: words('Abzweig\\p{L}*|Cache|CLI|Sitzung|Assistent'),
  };
  ok(jargon.es!.test('Crear rama desde aquí') && !jargon.es!.test('el programa de la IA'), 'a word inside another word is not jargon');
  const keys = ['answer.branchHere', 'mine.sendBranch', 'answer.cacheWarm', 'answer.terminalHint', 'kit.details', 'kit.line', 'error.driverStopped', 'error.sessionLost', 'note.historyResent', 'ask.question.label', 'projects.chat.lead'] as const;
  for (const { code } of LANGUAGES) {
    for (const key of keys) {
      const text = templates(key, code).other;
      ok(!jargon[code]!.test(text), `${code} ${key} reads plainly: ${text}`);
    }
  }
});

check('thread: prepending a message never rewrites an existing answer', async () => {
  const messages: Message[] = Array.from({ length: 6 }, (_, i) => ({
    id: `retained-${i}`, role: i % 2 ? 'assistant' : 'user', text: `Unique message ${i}`, at: 1,
    ...(i % 2 ? { status: 'done' as const } : {}),
  }));
  const [page, setPage] = createSignal(messages.slice(2));
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Thread, {
    get messages() { return page(); }, from: 0, conversationId: 'retained', options: [], onBranch: () => {},
  }), root);
  const answer = root.querySelector('.ai')!;
  const mutations: MutationRecord[] = [];
  const observer = new MutationObserver((records) => mutations.push(...records));
  observer.observe(answer.querySelector('.md')!, { subtree: true, childList: true, characterData: true });
  try {
    setPage(messages.slice(1));
    await Promise.resolve();
    equal(root.querySelector('[data-message-index="2"]'), answer, 'the same logical answer keeps its row');
    equal(mutations.length, 0, 'an earlier message never makes an existing answer display another message temporarily');
  } finally { observer.disconnect(); dispose(); root.remove(); }
});

check('thread: offscreen prompt containment preserves the full action hit target', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;top:100px;left:300px;width:400px;z-index:9999';
  document.body.append(root);
  const dispose = render(() => createComponent(Thread, {
    messages: [{ id: 'prompt', role: 'user', text: 'A prompt with actions', at: 1 }],
    from: 0, conversationId: 'prompt', options: [], onBranch: () => {},
  }), root);
  try {
    const button = root.querySelector<HTMLButtonElement>('.mine-bar button')!;
    button.focus();
    await frames();
    const box = button.getBoundingClientRect();
    ok(document.elementsFromPoint((box.left + box.right) / 2, box.bottom - 1).some((node) => node === button || button.contains(node)),
      'native containment never clips the bottom of the focusable action');
  } finally { dispose(); root.remove(); }
});
check('thread: skipped action glyphs reveal on keyboard focus and keep both exit fades', async () => {
  const motion = document.documentElement.dataset.motion;
  document.documentElement.dataset.motion = 'full';
  try {
    for (const role of ['mine', 'ai']) for (const kind of ['swap', 'i']) {
      const root = document.createElement('div');
      root.className = role;
      root.style.cssText = 'position:fixed;top:100px;left:300px;width:400px;z-index:9999;--dur-fast:100ms;--dur-feedback-max:150ms';
      const icon = '<svg class="i" width="14" height="14"></svg>';
      root.innerHTML = `<div class="${role === 'mine' ? 'bar mine-bar' : 'actions'}"><button class="iconbtn" aria-label="Copy">${kind === 'swap' ? `<span class="swap">${icon}${icon}</span>` : icon}</button></div>`;
      document.body.append(root);
      try {
        const button = root.querySelector('button')!;
        const glyph = button.firstElementChild!;
        const bar = role === 'mine' ? button.parentElement! : button;
        const visible = kind === 'swap' ? 'grid' : 'block';
        const touch = matchMedia('(hover: none)').matches;
        const focusState = () => `${role}/${kind}: document.hasFocus()=${document.hasFocus()}, :focus-within=${root.matches(':focus-within')}`;
        equal(getComputedStyle(button).contentVisibility, 'visible', 'the button itself never skips focus or hit testing');
        equal(getComputedStyle(glyph).display, visible, 'the glyph keeps its layout while hidden');
        equal(getComputedStyle(glyph).visibility, touch ? 'visible' : 'hidden', `the hover-free glyph starts hidden (${focusState()})`);
        button.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));
        button.focus({ preventScroll: true });
        const hasFocus = document.hasFocus();
        const focusStyles = hasFocus || root.matches(':focus-within');
        ok(document.activeElement === button, `focus reaches the keyboard target (${focusState()})`);
        if (focusStyles) {
          ok(root.matches(':focus-within'), `the row matches keyboard focus (${focusState()})`);
          ok(getComputedStyle(glyph).visibility === 'visible', `focus reveals the glyph before the animation clock advances (${focusState()})`);
        }
        if (!touch) {
          // Hidden WebKit can set activeElement without :focus-within; checking the rule also covers that inactive path.
          const rules = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules])
            .flatMap((rule) => rule instanceof CSSMediaRule ? [...rule.cssRules] : [rule])
            .filter((rule): rule is CSSStyleRule => rule instanceof CSSStyleRule);
          ok(rules.some((rule) => rule.selectorText.includes(':focus-within')
            && glyph.matches(rule.selectorText.replaceAll(':focus-within', ':is(*)'))
            && rule.style.visibility === 'visible' && rule.style.transitionDuration === '0s'),
          `the glyph has an immediate keyboard focus reveal rule (${focusState()})`);
        }
        await frames();
        ok(document.activeElement === button, `the skipped glyph keeps its keyboard target (${focusState()})`);
        equal(getComputedStyle(glyph).display, visible);
        if (focusStyles) ok(getComputedStyle(glyph).visibility === 'visible', `keyboard focus reveals the glyph (${focusState()})`);
        for (const animation of root.getAnimations({ subtree: true })) animation.finish();
        button.blur();
        void getComputedStyle(glyph).visibility;
        if (!touch) {
          equal(getComputedStyle(glyph).transitionProperty, 'visibility');
          equal(getComputedStyle(bar).transitionProperty, 'opacity');
          ok(parseFloat(getComputedStyle(glyph).transitionDuration) > 0, 'the glyph keeps a nonzero exit fade without window focus');
          equal(getComputedStyle(glyph).transitionDuration, getComputedStyle(bar).transitionDuration, 'glyph visibility and opacity keep equal exit durations');
        }
        if (!touch && focusStyles) {
          const exit = glyph.getAnimations().find((a) => a instanceof CSSTransition && a.transitionProperty === 'visibility');
          const fade = bar.getAnimations().find((a) => a instanceof CSSTransition && a.transitionProperty === 'opacity');
          ok(exit && fade, `${role} keeps both glyph visibility and opacity transitions (${focusState()})`);
          equal(exit.effect!.getComputedTiming().duration, fade.effect!.getComputedTiming().duration);
          exit.pause(); exit.currentTime = 50;
          equal(getComputedStyle(glyph).display, visible, 'the glyph survives halfway through the fade');
          equal(getComputedStyle(glyph).visibility, 'visible', 'the glyph stays visible halfway through the fade');
          exit.finish();
        }
        equal(getComputedStyle(glyph).display, visible);
        equal(getComputedStyle(glyph).visibility, touch ? 'visible' : 'hidden', `the glyph returns to its resting visibility (${focusState()})`);
      } finally { root.remove(); }
    }
  } finally {
    if (motion === undefined) delete document.documentElement.dataset.motion;
    else document.documentElement.dataset.motion = motion;
  }
});
check('thread: following the tail writes only when the scroll position changes', () => {
  const thread = document.createElement('div');
  let top = 400, writes = 0;
  Object.defineProperties(thread, {
    scrollHeight: { value: 500 }, clientHeight: { value: 100 },
    scrollTop: { get: () => top, set: (value: number) => { top = value; writes++; } },
  });
  followTail(thread);
  equal(writes, 0);
  top = 20;
  followTail(thread);
  equal(top, 400);
  equal(writes, 1);
  followTail(thread);
  equal(writes, 1);
});
