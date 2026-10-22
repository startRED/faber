import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type Conversation, type Message, type RewindPoint, type RewindPreview, type RewindRequest, type RewindResult } from '../../ui/api.ts';
import { ContextMenuHost } from '../../ui/ContextMenu.tsx';
import { installContextMenu } from '../../ui/contextmenu.ts';
import { lang, t, useLanguage } from '../../ui/i18n.ts';
import { Rewind } from '../../ui/Rewind.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const key = (element: Element, value: string, shiftKey = false) => element.dispatchEvent(new KeyboardEvent('keydown', { key: value, shiftKey, bubbles: true, cancelable: true }));
const buttons = (root: ParentNode) => [...root.querySelectorAll<HTMLButtonElement>('button')];
const button = (root: ParentNode, text: string) => buttons(document.querySelector('.rewind-confirm') ?? root).find((element) => element.textContent === text)!;
const dialog = (_root: ParentNode) => document.querySelector<HTMLDialogElement>('.rewind-confirm')!;
const go = (root: ParentNode) => button(root, t('rewind.go'));
const preview = (change: Partial<RewindPreview> = {}): RewindPreview => ({
  files: [{ path: 'notes.txt', action: 'replace', conflict: false, unavailable: false }],
  replacements: 1, recreations: 0, removals: 0, conflicts: 0, messages: 2,
  coverage: { saved: 1, skipped: 0, unavailable: false, incomplete: false }, running: [],
  recovery: false, redoExpiresAt: Date.now() + 86_400_000, notice: 'note.rewind.oldHandle', ...change,
});
const result = (change: Partial<RewindResult> = {}): RewindResult => ({
  id: 'redo-1', restored: ['notes.txt'], kept: [], failed: [], partial: false, recovery: false, notice: 'note.rewind.savedMessages', ...change,
});

function stage(motion = 'none') {
  const root = document.createElement('div');
  root.dataset.motion = motion;
  root.style.cssText = 'position:fixed;inset:0;width:720px;height:640px';
  for (const [name, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${name}`, value);
  document.body.append(root);
  return root;
}

function mocks() {
  const before = { rewindPoints: api.rewindPoints, rewindPreview: api.rewindPreview, rewindRestore: api.rewindRestore, rewindRedos: api.rewindRedos, rewindRedoPreview: api.rewindRedoPreview, rewindRedo: api.rewindRedo, rewindRecover: api.rewindRecover, interrupt: api.interrupt };
  api.rewindPoints = async () => [{ id: 'p1', messageId: 'u1', turnId: 't1', at: Date.now(), sealed: true, cursor: { entry: null, turn: null }, coverage: { saved: 1, skipped: 0, unavailable: false, incomplete: false } }];
  api.rewindPreview = async () => preview();
  api.rewindRestore = async () => result();
  api.rewindRedos = async () => [];
  api.rewindRedoPreview = async () => preview();
  api.rewindRedo = async () => result();
  api.rewindRecover = async () => {};
  api.interrupt = async () => {};
  return () => Object.assign(api, before);
}

function confirm(root: HTMLElement, options: { point?: RewindPoint; onClose?: () => void; onChanged?: () => void } = {}) {
  return render(() => createComponent(Rewind, { conversationId: 'rewind-check', messageId: 'u1', point: options.point, onClose: options.onClose ?? (() => {}), onChanged: options.onChanged ?? (() => {}) }), root);
}

const messages: Message[] = [
  { id: 'u1', role: 'user', text: 'Update the notes', at: 1 },
  { id: 'a1', role: 'assistant', text: 'The notes are updated.', at: 2, status: 'done' },
];

check('rewind: the existing message menu opens one modal confirm by keyboard and Cancel returns focus', async () => {
  const restore = mocks();
  installContextMenu();
  const root = stage();
  const dispose = render(() => [createComponent(Thread, { messages, from: 0, conversationId: 'rewind-check', options: [], onBranch: () => {} }), createComponent(ContextMenuHost, {})], root);
  try {
    const mine = root.querySelector<HTMLElement>('.mine')!;
    mine.focus();
    equal(document.activeElement, mine, 'messages are reachable without a pointer');
    key(mine, 'F10', true);
    await settle();
    const entries = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')];
    deepEqual(entries.map((entry) => entry.textContent), [t('menu.copy'), t('mine.edit'), t('rewind.title')]);
    entries[2]!.click();
    await settle();
    equal(document.querySelectorAll('.rewind-confirm').length, 1);
    ok(dialog(root).open, 'the native modal is open');
    ok(dialog(root).textContent!.includes(t('rewind.counts', { files: 1, messages: 2 })));
    ok(dialog(root).textContent!.includes(t('rewind.oldHandle')), 'the known outside-writer limit is stated');
    ok(dialog(root).contains(document.activeElement), 'focus moves into the confirm');
    ok(dialog(root).getAttribute('aria-labelledby'), 'a screen reader can name it');
    const last = button(root, t('rewind.cancel'));
    last.focus(); key(last, 'Tab');
    ok(dialog(root).contains(document.activeElement), 'Tab stays in the confirm');
    button(root, t('rewind.cancel')).click();
    await settle(260);
    equal(document.querySelector('.rewind-confirm'), null);
    equal(document.activeElement, mine);
  } finally { dispose(); root.remove(); restore(); }
});

check('rewind: all three scopes reach the API, both is the default and offline undo calls no AI', async () => {
  const restore = mocks();
  const send = api.send;
  api.send = async () => { throw new Error('undo must never ask an AI'); };
  try {
    for (const mode of ['both', 'files', 'conversation'] as const) {
      const root = stage();
      const calls: RewindRequest[] = [];
      api.rewindRestore = async (request) => { calls.push(request); return result(); };
      const dispose = confirm(root);
      try {
        await settle();
        const radios = [...dialog(root).querySelectorAll<HTMLInputElement>('input[name=rewind-scope]')];
        equal(radios.length, 3);
        equal(radios.find((radio) => radio.checked)?.value, 'both');
        radios.find((radio) => radio.value === mode)!.click();
        await settle();
        ok(!go(root).disabled, 'saved local messages and files need no connected AI');
        go(root).click();
        await settle();
        deepEqual(calls, [{ conversationId: 'rewind-check', messageId: 'u1', mode, savedVersions: [] }]);
      } finally { dispose(); root.remove(); }
    }
  } finally { api.send = send; restore(); }
});

check('rewind: conflicts keep my file by default and only explicitly chosen saved versions are restored', async () => {
  const restore = mocks();
  const root = stage();
  const calls: RewindRequest[] = [];
  api.rewindPreview = async () => preview({ conflicts: 1, files: [{ path: 'notes.txt', action: 'replace', conflict: true, unavailable: false }] });
  api.rewindRestore = async (request) => { calls.push(request); return result(); };
  const dispose = confirm(root);
  try {
    await settle();
    const file = dialog(root).querySelector<HTMLDetailsElement>('.rewind-files .change-file')!;
    ok(file, 'rewind reuses the Changes file preview');
    file.open = true;
    file.dispatchEvent(new Event('toggle'));
    await settle();
    const choice = file.querySelector<HTMLSelectElement>('select')!;
    equal(choice.getAttribute('aria-label'), 'notes.txt');
    equal(choice.value, 'keep');
    deepEqual([...choice.options].map((option) => option.value), ['keep', 'saved']);
    choice.value = 'saved';
    choice.dispatchEvent(new Event('change', { bubbles: true }));
    go(root).click();
    await settle();
    deepEqual(calls[0]?.savedVersions, ['notes.txt']);
  } finally { dispose(); root.remove(); restore(); }
});

check('rewind: preparing and restoring have static status and repeated Go back never repeats a restore', async () => {
  const restore = mocks();
  const root = stage();
  let ready!: (value: RewindPreview) => void;
  let finished!: (value: RewindResult) => void;
  let calls = 0;
  api.rewindPreview = () => new Promise((resolve) => { ready = resolve; });
  api.rewindRestore = () => { calls++; return new Promise((resolve) => { finished = resolve; }); };
  const dispose = confirm(root);
  try {
    await settle();
    ok(dialog(root).querySelector('[role=status]')?.textContent?.trim(), 'preparing is said in words');
    ok(go(root).disabled);
    ready(preview());
    await settle();
    const action = go(root);
    action.click(); action.click();
    await settle();
    equal(calls, 1);
    ok(dialog(root).querySelector('[role=status]')?.textContent?.trim(), 'restoring is said in words');
    finished(result());
    await settle();
  } finally { dispose(); root.remove(); restore(); }
});

check('rewind: waiting names running work and the existing Stop asks before interrupting and rechecks idle', async () => {
  const restore = mocks();
  const root = stage();
  let stopped = false;
  let previews = 0;
  const interrupted: string[] = [];
  api.rewindPreview = async () => { previews++; return preview({ running: stopped ? [] : [{ conversationId: 'other-conversation', writers: ['background build'], unknown: false }] }); };
  api.interrupt = async (id) => { interrupted.push(id); stopped = true; };
  const dispose = confirm(root);
  try {
    await settle();
    ok(dialog(root).textContent!.includes('other-conversation'));
    ok(dialog(root).textContent!.includes(t('rewind.writer.command')));
    ok(go(root).disabled);
    button(root, t('composer.stop')).click();
    await settle();
    deepEqual(interrupted, [], 'opening Stop never interrupts');
    ok(dialog(root).querySelector('.stop-ask'), 'reuse the existing Stop confirmation');
    button(root, t('composer.stopAll')).click();
    await settle();
    deepEqual(interrupted, ['other-conversation']);
    ok(previews >= 2, 'the engine is asked again before enabling restore');
    ok(!go(root).disabled);
  } finally { dispose(); root.remove(); restore(); }
});

check('rewind: preview and restore failures show their translated error key', async () => {
  const restore = mocks();
  try {
    for (const phase of ['preview', 'restore']) {
      const root = stage();
      api.rewindPreview = async () => { if (phase === 'preview') throw 'error.rewind.storage'; return preview(); };
      api.rewindRestore = async () => { throw 'error.rewind.storage'; };
      const dispose = confirm(root);
      try {
        await settle();
        if (phase === 'restore') { go(root).click(); await settle(); }
        equal(dialog(root).querySelector('[role=alert]')?.textContent, t('error.rewind.storage'));
      } finally { dispose(); root.remove(); }
    }
  } finally { restore(); }
});

check('rewind: unconfirmed Stop names the remaining background work in every language until exit is confirmed', async () => {
  const restore = mocks();
  const language = lang();
  try {
    for (const locale of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
      useLanguage(locale);
      const root = stage();
      let exited = false;
      const interrupted: string[] = [];
      api.rewindPreview = async () => preview({ running: exited ? [] : [{ conversationId: 'background-owner', writers: ['process-tree', 'process-tree:123'], unknown: true }] });
      api.interrupt = async (id) => { interrupted.push(id); };
      const dispose = confirm(root);
      try {
        await settle();
        button(root, t('composer.stop')).click();
        await settle();
        button(root, t('composer.stopAll')).click();
        await settle();
        deepEqual(interrupted, ['background-owner']);
        const text = dialog(root).textContent!;
        ok(text.includes('background-owner'), 'the remaining work belongs to a named conversation');
        for (const key of ['rewind.writer.background', 'error.rewind.stopUnconfirmed'] as const) {
          ok(t(key) !== key, `${locale} has words for ${key}`);
          ok(text.includes(t(key)), `${locale} explains what is still unconfirmed`);
        }
        ok(go(root).disabled, 'requesting Stop is not proof of exit');
        exited = true;
        button(root, t('rewind.refresh')).click();
        await settle();
        ok(!dialog(root).textContent!.includes(t('error.rewind.stopUnconfirmed')));
        ok(!go(root).disabled, 'only a fresh confirmed preview releases restore');
      } finally { dispose(); root.remove(); }
    }
  } finally { useLanguage(language); restore(); }
});

check('rewind: partial restore keeps the confirm open with recovery instead of claiming completion', async () => {
  const restore = mocks();
  const root = stage();
  let closed = 0;
  let recovered = 0;
  api.rewindRestore = async () => result({ partial: true, recovery: true, failed: ['notes.txt'] });
  api.rewindRecover = async () => { recovered++; };
  const dispose = confirm(root, { onClose: () => closed++ });
  try {
    await settle();
    go(root).click();
    await settle();
    equal(closed, 0);
    ok(dialog(root).open);
    const recovery = button(root, t('rewind.recover'));
    ok(recovery, 'a partial result gives an action to recover');
    recovery.click();
    await settle();
    equal(recovered, 1);
  } finally { dispose(); root.remove(); restore(); }
});

check('rewind: no changed files, unsaved file undo and expired file undo are distinct and conversation undo stays available', async () => {
  const restore = mocks();
  try {
    for (const state of ['empty', 'unsaved', 'expired'] as const) {
      const root = stage();
      const point: RewindPoint = { id: 'p1', messageId: 'u1', turnId: 't1', at: Date.now(), sealed: true, cursor: { entry: null, turn: null }, coverage: { saved: 0, skipped: 0, unavailable: state === 'unsaved', incomplete: false } };
      api.rewindPoints = async () => [{ ...point, coverage: { ...point.coverage, unavailable: state !== 'empty' } }];
      api.rewindPreview = async () => preview({ files: [], replacements: 0, coverage: { saved: 0, skipped: 0, unavailable: state !== 'empty', incomplete: false } });
      const dispose = confirm(root, { point });
      try {
        await settle();
        ok(dialog(root).querySelector('[role=status]')?.textContent?.includes(t(state === 'empty' ? 'rewind.noFiles' : state === 'unsaved' ? 'rewind.unavailable' : 'rewind.expired')), `${state} has its own static words`);
        dialog(root).querySelector<HTMLInputElement>('input[name=rewind-scope][value=conversation]')!.click();
        await settle();
        ok(!go(root).disabled, 'missing file history cannot disable saved messages');
      } finally { dispose(); root.remove(); }
    }
  } finally { restore(); }
});

check('rewind: redo reuses the confirmation and saved-message continuation stays a calm notice', async () => {
  const restore = mocks();
  const root = stage();
  installContextMenu();
  api.rewindRedos = async () => [{ id: 'redo-1', messageId: 'u1', mode: 'both', expiresAt: Date.now() + 86_400_000 }];
  let redone = 0;
  api.rewindRedo = async () => { redone++; return result(); };
  const dispose = render(() => [createComponent(Thread, { messages, from: 0, conversationId: 'rewind-check', options: [], rewindNotice: 'note.rewind.savedMessages', onBranch: () => {} }), createComponent(ContextMenuHost, {})], root);
  try {
    await settle();
    ok(root.textContent!.includes(t('note.rewind.savedMessages')));
    const mine = root.querySelector<HTMLElement>('.mine')!;
    mine.focus(); key(mine, 'ContextMenu');
    await settle();
    const redo = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].find((entry) => entry.textContent === t('rewind.redo'));
    ok(redo, 'a saved restore can be redone from the same menu');
    redo.click();
    await settle();
    ok(dialog(root).open);
    button(root, t('rewind.redo')).click();
    await settle();
    equal(redone, 1);
  } finally { dispose(); root.remove(); restore(); }
});

check('rewind: restore waits for the conversation reload before releasing controls and an empty thread keeps Redo', async () => {
  const restore = mocks();
  installContextMenu();
  const root = stage();
  const [shown, setShown] = createSignal(messages);
  const [notice, setNotice] = createSignal<string>();
  let reloading = false;
  let loaded!: () => void;
  let restored = false;
  api.rewindRestore = async () => { restored = true; return result(); };
  api.rewindRedos = async () => restored ? [{ id: 'redo-1', messageId: 'u1', mode: 'both', expiresAt: Date.now() + 86_400_000 }] : [];
  const dispose = render(() => [createComponent(Thread, {
    get messages() { return shown(); }, get rewindNotice() { return notice(); }, from: 0, conversationId: 'rewind-check', options: [], onBranch: () => {},
    onRewind: async () => { reloading = true; await new Promise<void>((resolve) => { loaded = resolve; }); setShown([]); setNotice('note.rewind.savedMessages'); },
  }), createComponent(ContextMenuHost, {})], root);
  try {
    const mine = root.querySelector<HTMLElement>('.mine')!;
    mine.focus(); key(mine, 'ContextMenu'); await settle();
    const entry = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].find((item) => item.textContent === t('rewind.title'))!;
    entry.click(); await settle(); go(root).click(); await settle();
    ok(reloading, 'restoring asks the caller to reload saved messages');
    ok(button(root, t('rewind.cancel')).disabled, 'the confirm waits for the reload promise');
    ok(root.querySelector('.mine'), 'the previous messages stay until the new record arrives');
    loaded(); await settle();
    equal(root.querySelector('.mine'), null, 'messages removed by undo disappear');
    ok(root.querySelector('.rewind-empty')?.textContent?.includes(t('note.rewind.savedMessages')));
    ok(!button(root, t('rewind.cancel')).disabled);
    button(root, t('rewind.cancel')).click(); await settle(260);
    const empty = root.querySelector<HTMLElement>('.rewind-empty')!;
    empty.focus(); key(empty, 'ContextMenu'); await settle();
    const entries = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')];
    deepEqual(entries.map((item) => item.textContent), [t('rewind.redo')]);
    key(entries[0]!, 'Escape');
  } finally { dispose(); root.remove(); restore(); }
});

check('rewind: an interrupted restore found after restart offers recovery even when its preview fails', async () => {
  const restore = mocks();
  installContextMenu();
  const root = stage();
  let recovered = 0;
  let reloaded = 0;
  api.rewindRedos = async () => [{ id: 'recovery-1', messageId: 'u1', mode: 'both', expiresAt: Date.now() + 86_400_000, recovery: true }];
  api.rewindRedoPreview = async () => { throw 'error.rewind.recovery'; };
  api.rewindRecover = async (id) => { equal(id, 'rewind-check'); recovered++; };
  const dispose = render(() => [createComponent(Thread, { messages: [], from: 0, conversationId: 'rewind-check', options: [], rewindNotice: 'note.rewind.savedMessages', onBranch: () => {}, onRewind: async () => { reloaded++; } }), createComponent(ContextMenuHost, {})], root);
  try {
    await settle();
    const empty = root.querySelector<HTMLElement>('.rewind-empty')!;
    empty.focus(); key(empty, 'ContextMenu'); await settle();
    const entries = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')];
    deepEqual(entries.map((item) => item.textContent), [t('rewind.recover')]);
    entries[0]!.click(); await settle();
    ok(dialog(root).open);
    ok(dialog(root).querySelector('[role=alert]')?.textContent?.includes(t('error.rewind.recovery')));
    ok(dialog(root).querySelector('[role=status]')?.textContent?.includes(t('rewind.recovery')));
    button(root, t('rewind.recover')).click(); await settle();
    equal(recovered, 1);
    equal(reloaded, 1, 'recovery reloads the saved conversation too');
    equal(button(root, t('rewind.recover')), undefined, 'successful recovery removes its action');
    ok(dialog(root).querySelector('[role=status]')?.textContent?.includes(t('rewind.partial')), 'recovery without a full result does not claim all files were restored');
  } finally { dispose(); root.remove(); restore(); }
});

check('rewind: the app reloads after undoing the first message and keeps the saved conversation instead of the new-chat home', async () => {
  const restore = mocks();
  installContextMenu();
  const root = stage();
  const before = { open: api.open, remove: api.remove, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, notesSeen: api.notesSeen, updateStatus: api.updateStatus, onUpdate: api.onUpdate };
  const storage = localStorage.getItem('faber.place');
  const language = lang();
  let conversation: Conversation = { id: 'rewind-check', title: 'Saved notes', area: 'chat', sessions: {}, createdAt: 1, updatedAt: 2, messages };
  const initial = {
    conversations: [{ id: conversation.id, title: conversation.title, area: 'chat', updatedAt: 2 }], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test',
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false }, appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
  } as AppState;
  let opened = 0;
  let refused = 0;
  api.open = async () => { opened++; return structuredClone(conversation); };
  api.remove = async () => { refused++; throw 'error.rewind.deleteRecovery'; };
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.notesSeen = async () => Number.MAX_SAFE_INTEGER;
  api.updateStatus = async () => ({ phase: 'none' });
  api.onUpdate = async () => () => {};
  api.rewindPreview = async () => {
    if (!conversation.messages.some((message) => message.id === 'u1')) throw 'error.rewind.message';
    return preview();
  };
  api.rewindRestore = async () => { conversation = { ...conversation, messages: [], rewindNotice: 'note.rewind.savedMessages' }; return result(); };
  api.rewindRedos = async () => conversation.rewindNotice ? [{ id: 'redo-1', messageId: 'u1', mode: 'both', expiresAt: Date.now() + 86_400_000 }] : [];
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: conversation.id }));
  const dispose = render(() => [createComponent(App, { initial, inbox: { take: () => {} } }), createComponent(ContextMenuHost, {})], root);
  try {
    await settle();
    const mine = root.querySelector<HTMLElement>('.mine')!;
    ok(mine, 'the saved conversation opened');
    for (const language of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
      useLanguage(language);
      ok(t('error.rewind.deleteRecovery') !== 'error.rewind.deleteRecovery', `${language}: the refusal has plain words`);
      root.querySelector('.crow')!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2, clientX: 40, clientY: 40 }));
      await settle();
      const remove = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].find((item) => item.textContent === t('menu.delete'))!;
      remove.click(); await settle(560);
      remove.click();
      // The refusal reaches the screen through an engine call and a render: wait for it, not a fixed time (a slow macOS runner takes longer).
      for (const start = Date.now(); !root.querySelector('[role=alert]')?.textContent?.includes(t('error.rewind.deleteRecovery')) && Date.now() - start < 3000; ) await settle(50);
      ok(root.querySelector('[role=alert]')?.textContent?.includes(t('error.rewind.deleteRecovery')), `${language}: recovery refusal uses the existing error display`);
      ok(root.querySelector('.crow'), 'a refused deletion keeps the conversation listed');
      ok(root.querySelector('.mine'), 'a refused deletion keeps the current conversation open');
    }
    equal(refused, 5);
    useLanguage('en');
    mine.focus(); key(mine, 'ContextMenu'); await settle();
    const entry = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].find((item) => item.textContent === t('rewind.title'))!;
    entry.click(); await settle(); go(root).click(); await settle();
    ok(opened >= 2, 'the app reads the conversation again after undo');
    equal(root.querySelector('.mine'), null);
    ok(root.querySelector('.rewind-empty')?.textContent?.includes(t('note.rewind.savedMessages')));
    equal(root.querySelector('.welcome-grid'), null, 'the empty saved conversation is not a new chat');
    equal(dialog(root).querySelector('[role=alert]'), null, 'reloading the saved conversation never previews its removed message again');
  } finally {
    dispose(); root.remove(); Object.assign(api, before); restore(); useLanguage(language);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('rewind: incomplete file history is marked on its message and an empty restored conversation retains the redo menu', async () => {
  const restore = mocks();
  installContextMenu();
  try {
    for (const empty of [false, true]) {
      const root = stage();
      const point: RewindPoint = { id: 'p1', messageId: 'u1', turnId: 't1', at: 1, sealed: true, cursor: { entry: null, turn: null }, coverage: { saved: 1, skipped: 1, incomplete: true, unavailable: false } };
      api.rewindRedos = async () => [{ id: 'redo-1', messageId: 'u1', mode: 'both', expiresAt: Date.now() + 86_400_000 }];
      const dispose = render(() => [createComponent(Thread, { messages: empty ? [] : [{ ...messages[0]!, rewind: point }], from: 0, conversationId: 'rewind-check', options: [], rewindNotice: 'note.rewind.savedMessages', onBranch: () => {} }), createComponent(ContextMenuHost, {})], root);
      try {
        await settle();
        if (empty) {
          const notice = root.querySelector<HTMLElement>('.rewind-empty')!;
          notice.focus(); key(notice, 'ContextMenu');
          await settle();
          const entries = [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')];
          deepEqual(entries.map((entry) => entry.textContent), [t('rewind.redo')]);
          key(entries[0]!, 'Escape');
        } else {
          equal(root.querySelector('.mine .rewind-coverage')?.textContent, t('rewind.coverage'));
        }
      } finally { dispose(); root.remove(); }
    }
  } finally { restore(); }
});

check('rewind: a 720 px window fits German paths and controls in reduced and no motion with static state', async () => {
  const restore = mocks();
  const language = lang();
  const width = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
  const previousMotion = document.documentElement.dataset.motion;
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 720 });
  useLanguage('de');
  api.rewindPreview = async () => preview({ coverage: { saved: 1, skipped: 3, incomplete: true, unavailable: false }, conflicts: 1, files: [{ path: 'a-very-long-folder-name/'.repeat(7) + 'notes.txt', action: 'replace', conflict: true, unavailable: false }] });
  try {
    for (const motion of ['reduced', 'none']) {
      document.documentElement.dataset.motion = motion;
      const root = stage(motion);
      const dispose = confirm(root);
      try {
        await settle();
        const modal = dialog(root);
        ok(modal.scrollWidth <= modal.clientWidth + 1, 'long paths must not scroll sideways');
        ok(modal.getBoundingClientRect().width <= 720);
        ok(modal.textContent!.trim().length > 0, 'coverage is visible without animation');
        equal(modal.getAnimations({ subtree: true }).filter((animation) => animation.playState === 'running').length, 0);
        ok(button(root, t('rewind.cancel')).getBoundingClientRect().width > 0);
      } finally { dispose(); root.remove(); }
    }
  } finally {
    Object.defineProperty(window, 'innerWidth', width);
    if (previousMotion === undefined) delete document.documentElement.dataset.motion;
    else document.documentElement.dataset.motion = previousMotion;
    useLanguage(language); restore();
  }
});
