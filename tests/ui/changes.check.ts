import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState, type Conversation, type FileChange } from '../../ui/api.ts';
import { Changes } from '../../ui/Changes.tsx';
import { store } from '../../ui/running.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { animateDetails } from '../../ui/motion.ts';
import recorded from '../fixtures/conversation-changes.json';
import { check, deepEqual, equal, ok } from './check.ts';

const look = {
  vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
  appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
};

check('review change: both requests use the conversation send path while idle or working, never in Chat', async () => {
  useLanguage('en');
  const before = { open: api.open, send: api.send, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, changes: api.changes };
  const storage = localStorage.getItem('faber.place');
  const revision = store.list().revision;
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    for (const busy of [false, true]) {
      const c = structuredClone(recorded.conversation) as Conversation;
      const initial = {
        conversations: [{ id: c.id, title: c.title, area: c.area, projectId: 'p1', updatedAt: c.updatedAt }],
        projects: [{ id: 'p1', name: 'Greeting', path: 'E:/greeting' }], vendors: ['claude'],
        settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
        running: { revision: revision + 1, items: busy ? [{ conversationId: c.id, turnId: 'working', title: c.title, area: 'project', projectId: 'p1', state: 'working', since: 0, permissions: [] }] : [] },
        quotas: [], usage: [], models: [], version: 'test', look,
      } as AppState;
      if (busy) c.messages.at(-1)!.status = 'streaming';
      let sent: Parameters<typeof api.send>[0] | undefined;
      api.open = async () => c; api.state = async () => initial;
      api.warm = async () => {}; api.routeDraft = async () => {};
      api.setClock = async () => {}; api.setShellTexts = async () => {};
      api.changes = async () => ({ files: recorded.changes as FileChange[], incomplete: false });
      api.send = async (request) => { sent = request; return c.id; };
      localStorage.setItem('faber.place', JSON.stringify({ area: 'project', projectId: 'p1', id: c.id }));
      store.offer({ revision: store.list().revision + 1, items: initial.running.items });
      const host = document.createElement('div');
      document.body.append(host);
      let hear!: (event: AppEvent) => void;
      const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
      try {
        await settle();
        host.querySelector<HTMLButtonElement>('.run-btn')!.click();
        await settle();
        for (const security of [false, true]) {
          host.querySelector<HTMLButtonElement>('.rc-changes')!.click();
          await settle();
          const buttons = host.querySelectorAll<HTMLButtonElement>('.changes-review button');
          equal(buttons.length, 2, 'the changes head offers both reviews');
          equal(buttons[0]!.textContent, 'Review');
          equal(buttons[1]!.textContent, 'Review with security');
          buttons[security ? 1 : 0]!.click();
          await settle();
          ok(sent, 'a review calls the same api.send as the composer');
          equal(sent.conversationId, c.id); equal(sent.area, 'project'); equal(sent.projectId, 'p1');
          equal(sent.text, t(security ? 'changes.reviewSecurityRequest' : 'changes.reviewRequest'));
          ok(sent.text.includes(security ? 'for bugs and security issues.' : 'for bugs.'));
          for (const words of ['copy base', 'helper tool', 'other AI when it is connected', 'fresh read-only subagent', 'short list, worst first', 'Do not change files', 'no findings']) ok(sent.text.includes(words), words);
          deepEqual(sent.images, []); deepEqual(sent.files, []);
          equal(host.querySelector('.changes-view'), null, 'sending returns to the running list');
          ok(host.querySelector('.rc-body:not(.changes-view)'), 'the running list is back');
          sent = undefined;
        }
        // No project control is allowed to leak into a Chat conversation, even with file tools in its history.
        c.area = 'chat'; delete c.projectId;
        hear({ type: 'message', conversationId: c.id, message: { id: 'chat-answer', role: 'assistant', text: 'Done.', at: Date.now(), status: 'done' } });
        await settle();
        equal(host.querySelector('.rc-changes'), null);
        equal(host.querySelector('.changes-review'), null);
      } finally { dispose(); host.remove(); }
    }
  } finally {
    Object.assign(api, before);
    store.offer({ revision: store.list().revision + 1, items: [] });
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('review change: switching conversations hides both review buttons before the next conversation loads', async () => {
  useLanguage('en');
  const before = { open: api.open, send: api.send, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, changes: api.changes };
  const storage = localStorage.getItem('faber.place');
  const width = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1920 });
  window.dispatchEvent(new Event('resize'));
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const c = structuredClone(recorded.conversation) as Conversation;
  const next = { ...structuredClone(c), id: 'next-review-project', title: 'Second conversation' };
  const initial = {
    conversations: [c, next].map(({ id, title, area, projectId, updatedAt }) => ({ id, title, area, projectId, updatedAt })),
    projects: [{ id: 'p1', name: 'Greeting', path: 'E:/greeting' }], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  const sent: Parameters<typeof api.send>[0][] = [];
  let finishOpen!: (conversation: Conversation) => void;
  let openingNext = false;
  const pending = new Promise<Conversation>((resolve) => { finishOpen = resolve; });
  api.open = async (id) => {
    if (id === next.id) { openingNext = true; return pending; }
    return c;
  };
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.changes = async () => ({ files: recorded.changes as FileChange[], incomplete: false });
  api.send = async (request) => { sent.push(request); return request.conversationId!; };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'project', projectId: 'p1', id: c.id }));
  store.offer({ revision: store.list().revision + 1, items: [] });
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
  try {
    await settle();
    host.querySelector<HTMLButtonElement>('.run-btn')!.click();
    await settle();
    host.querySelector<HTMLButtonElement>('.rc-changes')!.click();
    await settle();
    const buttons = [...host.querySelectorAll<HTMLButtonElement>('.changes-review button')];
    equal(buttons.length, 2, 'both reviews are available for the loaded conversation');
    const row = [...host.querySelectorAll<HTMLButtonElement>('.side .item')].find((button) => button.textContent?.includes(next.title));
    ok(row, 'the second conversation is selectable in the sidebar at desktop width');
    row.click();
    await settle();
    ok(openingNext, 'api.open of the second conversation stays pending');
    equal(host.querySelector('.side .item[aria-current="true"]')?.textContent?.trim(), next.title);
    // A click at either former review control cannot send to the old or the opening conversation.
    for (const button of buttons) button.click();
    await settle();
    deepEqual(sent, [], 'no review reaches either conversation during the switch');
    equal(host.querySelector('.changes-review'), null, 'no review buttons remain while the conversation loads');
    finishOpen(next);
    await settle();
    equal(host.querySelector('.changes-review'), null, 'the old changes view cannot review the newly opened conversation');
    deepEqual(sent, []);
  } finally {
    finishOpen(next);
    dispose(); host.remove(); Object.assign(api, before);
    store.offer({ revision: store.list().revision + 1, items: [] });
    Object.defineProperty(window, 'innerWidth', width);
    window.dispatchEvent(new Event('resize'));
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('review change: no changed files means no review buttons, including an incomplete empty page', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const [files, setFiles] = createSignal<FileChange[]>([]);
  const [incomplete, setIncomplete] = createSignal(false);
  let sent = false;
  const dispose = render(() => createComponent(Changes, {
    id: 'review-empty', get files() { return files(); }, loading: false, get incomplete() { return incomplete(); },
    onReview: () => { sent = true; }, onBack: () => {}, onClose: () => {},
  }), host);
  try {
    equal(host.querySelector('.changes-review'), null);
    setIncomplete(true);
    equal(host.querySelector('.changes-review'), null);
    setFiles(recorded.changes as FileChange[]);
    equal(host.querySelectorAll('.changes-review button').length, 2, 'the guard is exercised with a real callback');
    setFiles([]);
    equal(host.querySelector('.changes-review'), null, 'the buttons disappear if changes go away');
    equal(sent, false);
  } finally { dispose(); host.remove(); }
});

check('conversation changes: full motion cannot reopen A after opening B and closing B during A entrance', async () => {
  const preference = Object.getOwnPropertyDescriptor(MediaQueryList.prototype, 'matches')!;
  Object.defineProperty(MediaQueryList.prototype, 'matches', { ...preference, get(this: MediaQueryList) {
    return this.media === '(prefers-reduced-motion: reduce)' ? false : preference.get!.call(this);
  } });
  const host = document.createElement('div');
  host.dataset.motion = 'full';
  document.body.append(host);
  const before = api.changePatch;
  api.changePatch = async () => ({ text: '@@ -1 +1 @@\n-old\n+new\n', tooLarge: false, binary: false });
  const files = ['a.txt', 'b.txt'].map((path): FileChange => ({ path, added: 1, removed: 1, binary: false, status: 'modified' }));
  const dispose = render(() => {
    animateDetails(host);
    return createComponent(Changes, { id: 'motion-review', files, loading: false, onBack: () => {}, onClose: () => {} });
  }, host);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 10));
  try {
    // An earlier window-size check can temporarily suspend all full-motion entrances.
    await new Promise((resolve) => setTimeout(resolve, 260));
    const [a, b] = [...host.querySelectorAll<HTMLDetailsElement>('.change-file')];
    a!.querySelector('summary')!.click();
    const entrance = a!.getAnimations()[0]!;
    ok(entrance, 'the check must exercise full motion');
    entrance.pause();
    await settle();
    b!.querySelector('summary')!.click();
    await settle();
    equal(a!.open, false, 'opening B closes A before its entrance finishes');
    b!.querySelector('summary')!.click();
    for (const animation of b!.getAnimations()) animation.finish();
    await settle();
    // Simulate the late entrance completion that used to resurrect A.
    if (entrance.playState !== 'idle') entrance.finish();
    await settle();
    equal(host.querySelectorAll('.change-file[open]').length, 0);
    equal(host.querySelector('.change-patch'), null, 'no hidden or resurrected patch remains mounted');
    equal(entrance.playState, 'idle', 'closing A cancels its entrance');
  } finally {
    dispose(); host.remove(); api.changePatch = before;
    Object.defineProperty(MediaQueryList.prototype, 'matches', preference);
  }
});

check('conversation changes: failed and timed-out patch reads show a retry instead of an empty change', async () => {
  useLanguage('en');
  const before = api.changePatch;
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    for (const rejected of [false, true]) {
      const host = document.createElement('div');
      host.dataset.motion = 'none';
      document.body.append(host);
      let reads = 0;
      api.changePatch = async () => {
        if (++reads === 1) {
          if (rejected) throw new Error('transport failed');
          return { text: '', binary: false, tooLarge: false, error: 'error.changes.read' };
        }
        return { text: '@@ -1 +1 @@\n-old\n+loaded\n', binary: false, tooLarge: false };
      };
      const dispose = render(() => createComponent(Changes, {
        id: 'failed-review', files: [{ path: 'report.txt', added: 1, removed: 1, binary: false, status: 'modified' }], loading: false, onBack: () => {}, onClose: () => {},
      }), host);
      try {
        host.querySelector('summary')!.click();
        await settle();
        ok(host.textContent!.includes(t('error.changes.read')));
        ok(!host.textContent!.includes(t('changes.empty')), 'an error must never claim there are no changes');
        const retry = [...host.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === t('changes.retry'));
        ok(retry, 'the same open file offers a retry');
        retry.click();
        await settle();
        equal(reads, 2);
        ok(host.querySelector('.patch-line.add')!.textContent!.includes('+loaded'));
        ok(!host.textContent!.includes(t('error.changes.read')));
      } finally {
        dispose(); host.remove();
      }
    }
  } finally {
    api.changePatch = before;
  }
});

check('conversation changes: at 720 px, the auto-opened column becomes modal, pages files and loads lines lazily', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { open: api.open, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, changes: api.changes, changePatch: api.changePatch };
  const storage = localStorage.getItem('faber.place');
  const width = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 720 });
  window.dispatchEvent(new Event('resize'));
  const c = structuredClone(recorded.conversation) as Conversation;
  const initial = {
    conversations: [{ id: c.id, title: c.title, area: c.area, projectId: 'p1', updatedAt: c.updatedAt }],
    projects: [{ id: 'p1', name: 'Greeting', path: 'E:/greeting' }], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 0, items: [{ conversationId: c.id, turnId: 'waiting', title: c.title, area: 'project', projectId: 'p1', state: 'needsYou', since: 0, permissions: [{ requestId: 'pending', tool: 'Bash', input: {} }] }] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  let listReads = 0;
  let files = recorded.changes as FileChange[];
  const patchReads: string[] = [];
  api.open = async () => c;
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  const cursors: Array<string | undefined> = [];
  api.changes = async (id, cursor) => {
    equal(id, c.id); listReads++; cursors.push(cursor);
    return cursor === 'next' ? { files, incomplete: false } : { files: files.slice(0, 1), incomplete: files.length > 1, next: files.length > 1 ? 'next' : undefined };
  };
  api.changePatch = async (id, path) => {
    equal(id, c.id);
    patchReads.push(path);
    return { text: recorded.patches[path as keyof typeof recorded.patches], binary: false, tooLarge: false };
  };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'project', projectId: 'p1', id: c.id }));
  // The column reads the running list from its store, which main.tsx fills from the snapshot.
  const revision = store.list().revision;
  store.offer({ revision: revision + 1, items: initial.running.items });
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  const disclosures = () => [...host.querySelectorAll<HTMLDetailsElement>('.change-file')];
  try {
    await settle();
    const automatic = host.querySelector<HTMLElement>('#running');
    ok(automatic, 'the pending request opens the column without a click');
    equal(automatic.getAttribute('aria-modal'), null, 'the automatic column does not capture focus');
    ok(!host.querySelector<HTMLElement>('.dock')!.inert, 'the composer is reachable before opening a side view');
    const control = host.querySelector<HTMLButtonElement>('.rc-changes');
    ok(control, 'a project conversation with changes offers the single control in the column head');
    equal(control!.closest('.rc-head')?.parentElement?.id, 'running');
    control!.click();
    await settle();
    const modal = host.querySelector<HTMLElement>('#running')!;
    equal(modal.getAttribute('aria-modal'), 'true', 'Changes uses the same modal transition as the other side views');
    ok(modal.contains(document.activeElement), 'focus moves inside the column');
    for (const selector of ['.thread', '.dock', '.topbar', '.side']) ok(host.querySelector<HTMLElement>(selector)!.inert, `${selector} stays behind the modal`);
    const back = modal.querySelector<HTMLButtonElement>('.back')!;
    back.focus();
    const tab = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true });
    back.dispatchEvent(tab);
    ok(tab.defaultPrevented && modal.contains(document.activeElement) && document.activeElement !== back, 'Shift+Tab wraps inside the existing focus trap');
    equal(host.querySelectorAll('#running').length, 1, 'the existing column is reused');
    equal(host.querySelector('.rc-body:not(.changes-view)'), null, 'the running list makes way for the changes');
    equal(disclosures().length, 1, 'the files already found stay visible on an incomplete page');
    ok(host.textContent!.includes(t('changes.incomplete')));
    host.querySelector<HTMLButtonElement>('.changes-more')!.click();
    await settle();
    deepEqual(cursors, [undefined, 'next'], 'Show more sends the continuation instead of repeating the first page');
    equal(disclosures().length, 2, 'continuation appends and deduplicates files already found');
    equal(host.querySelector('.changes-more'), null, 'a complete list needs no continuation');
    for (const f of files) {
      const row = disclosures().find((d) => d.querySelector('summary')!.textContent!.includes(f.path));
      ok(row, `${f.path} is listed`);
      ok(row!.querySelector('.delta')!.textContent!.includes(`+${f.added}`), 'the added count is visible');
      if (f.removed) ok(row!.querySelector('.delta')!.textContent!.includes(`−${f.removed}`), 'the removed count is visible');
    }
    deepEqual(patchReads, [], 'listing the files reads no patches');
    const source = disclosures().find((d) => d.textContent!.includes('src/greeting.ts'))!;
    source.querySelector('summary')!.click();
    await settle();
    deepEqual(patchReads, ['src/greeting.ts'], 'only the opened file is read');
    ok(source.querySelector('.patch-line.del')!.textContent!.includes("-export const greeting = 'Hello';"));
    ok(source.querySelector('.patch-line.add')!.textContent!.includes("+export const greeting = 'Welcome';"));
    equal(source.querySelector('script'), null, 'patches are text');
    source.querySelector('summary')!.click();
    await settle();
    equal(source.querySelector('.change-patch'), null, 'closing releases the patch DOM');
    source.querySelector('summary')!.click();
    await settle();
    deepEqual(patchReads, ['src/greeting.ts', 'src/greeting.ts'], 'a reopened file is read afresh');
    host.querySelector<HTMLButtonElement>('.changes-view .back, .side-head .back')!.click();
    await settle();
    ok(host.querySelector('.rc-body'), 'back returns to the running list');
    equal(host.querySelector('.change-patch'), null);
    const beforeTurn = listReads;
    files = [];
    hear({ type: 'message', conversationId: c.id, message: { id: 'r2', role: 'assistant', at: Date.now(), text: '', status: 'streaming' } });
    hear({ type: 'message', conversationId: c.id, message: { id: 'r2', role: 'assistant', at: Date.now(), text: 'No changes left.', status: 'done' } });
    await settle();
    ok(listReads > beforeTurn, 'the end of a turn refreshes git counts');
    equal(host.querySelector('.rc-changes'), null, 'nothing changed leaves no control');
  } finally {
    dispose(); host.remove(); Object.assign(api, before);
    store.offer({ revision: revision + 2, items: [] });
    Object.defineProperty(window, 'innerWidth', width);
    window.dispatchEvent(new Event('resize'));
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('conversation changes: opening thirty 3990-line files keeps only one patch and measures its DOM budget', async () => {
  const host = document.createElement('div');
  document.body.append(host);
  const before = api.changePatch;
  const files = Array.from({ length: 30 }, (_, i): FileChange => ({ path: `file-${i}.txt`, added: 3989, removed: 0, binary: false, status: 'modified' }));
  const lineCount = 3990;
  const text = `@@ -0,0 +1,3989 @@\n${Array.from({ length: lineCount - 1 }, (_, i) => `+line ${i}`).join('\n')}\n`;
  let reads = 0;
  api.changePatch = async () => { reads++; return { text, tooLarge: false, binary: false }; };
  const dispose = render(() => createComponent(Changes, { id: 'large-review', files, loading: false, onBack: () => {}, onClose: () => {} }), host);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const countNodes = () => {
    const walker = document.createTreeWalker(host, NodeFilter.SHOW_ALL);
    let count = 0;
    while (walker.nextNode()) count++;
    return count;
  };
  const baseline = { elements: host.querySelectorAll('*').length, nodes: countNodes() };
  let peakElements = 0;
  let peakNodes = 0;
  const started = performance.now();
  try {
    const rows = [...host.querySelectorAll<HTMLDetailsElement>('.change-file')];
    for (const row of rows) {
      row.querySelector('summary')!.click();
      await settle();
      equal(host.querySelectorAll('.change-file[open]').length, 1, 'opening a new file closes the previous disclosure');
      equal(host.querySelectorAll('.change-patch').length, 1);
      equal(host.querySelectorAll('.patch-line').length, lineCount, 'line DOM does not accumulate across files');
      peakElements = Math.max(peakElements, host.querySelectorAll('*').length);
      peakNodes = Math.max(peakNodes, countNodes());
      ok(peakElements <= baseline.elements + lineCount + 2, `only the current patch adds elements: ${peakElements}`);
      ok(peakNodes <= baseline.nodes + 4 * lineCount + 50, `text and comment nodes stay bounded too: ${peakNodes}`);
    }
    equal(reads, 30, 'all thirty files were opened, not just the last one');
    console.info(`Conversation changes DOM: 30 x ${lineCount} lines; peak ${peakElements} elements, ${peakNodes} total nodes; ${Math.round(performance.now() - started)} ms including settling.`);
    rows.at(-1)!.querySelector('summary')!.click();
    await settle();
    equal(host.querySelectorAll('.patch-line').length, 0, 'closing the final file releases every patch line');
    equal(host.querySelectorAll('*').length, baseline.elements);
  } finally {
    dispose(); host.remove(); api.changePatch = before;
  }
});

check('conversation changes: loading, empty, binary and capped files have words without invented counts', async () => {
  useLanguage('en');
  const host = document.createElement('div');
  host.style.width = '300px';
  document.body.append(host);
  const before = api.changePatch;
  const [loading, setLoading] = createSignal(true);
  const [incomplete, setIncomplete] = createSignal(false);
  const [files, setFiles] = createSignal<FileChange[]>([]);
  api.changePatch = async (_id, path) => path === 'small/after-budget.txt'
    ? { text: '@@ -0,0 +1 @@\n+small\n', binary: false, tooLarge: false }
    : { text: '', binary: false, tooLarge: true };
  const dispose = render(() => createComponent(Changes, {
    id: 'capped', get files() { return files(); }, get loading() { return loading(); }, get incomplete() { return incomplete(); }, onBack: () => {}, onClose: () => {},
  }), host);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    ok(host.textContent!.includes(t('changes.loading')));
    setLoading(false);
    ok(host.textContent!.includes(t('changes.empty')));
    setIncomplete(true);
    ok(host.textContent!.includes(t('changes.incomplete')));
    ok(!host.textContent!.includes(t('changes.empty')), 'an incomplete list never claims there are no changes');
    ok(host.querySelector('.changes-more'), 'an empty partial page still lets the person continue');
    setIncomplete(false);
    setFiles([
      { path: 'long/path/image.png', added: 0, removed: 0, binary: true, status: 'modified' },
      { path: 'large/new.txt', added: 0, removed: 0, binary: false, countsKnown: false, status: 'added' },
      { path: 'large/tracked.txt', added: 5000, removed: 2, binary: false, status: 'modified', tooLarge: true },
      { path: 'large/patch.txt', added: 6000, removed: 0, binary: false, status: 'modified' },
      { path: 'small/after-budget.txt', added: 0, removed: 0, binary: false, countsKnown: false, tooLarge: false, status: 'added' },
    ]);
    const rows = [...host.querySelectorAll<HTMLDetailsElement>('.change-file')];
    ok(rows[0]!.textContent!.includes(t('changes.binary')));
    equal(rows[0]!.querySelector('.delta'), null, 'binary has no line numbers');
    ok(rows[1]!.textContent!.includes(t('changes.countsUnknown')));
    equal(rows[1]!.querySelector('.delta'), null, 'capped untracked files never claim zero lines');
    rows[2]!.querySelector('summary')!.click();
    await settle();
    ok(rows[2]!.textContent!.includes(t('changes.tooLarge')));
    ok(rows[2]!.textContent!.includes('+5000'), 'tracked counts remain when its patch is too large');
    equal(host.querySelector('.change-patch'), null, 'a capped patch keeps no line DOM');
    rows[3]!.querySelector('summary')!.click();
    await settle();
    ok(rows[3]!.textContent!.includes(t('changes.tooLarge')), 'the patch response can also report its cap');
    rows[4]!.querySelector('summary')!.click();
    await settle();
    equal(rows[4]!.querySelector('.delta'), null, 'unknown counts never invent zero lines');
    ok(rows[4]!.querySelector('.patch-line.add')!.textContent!.includes('+small'), 'a small file beyond the counting budget still opens');
  } finally {
    dispose(); host.remove(); api.changePatch = before;
  }
});
