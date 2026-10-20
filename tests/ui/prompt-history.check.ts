import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer, type Draft } from '../../ui/Composer.tsx';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type Area, type Command, type Conversation, type Mention } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// The hidden test window does not deliver focus events; drive them as the idle checks do.
function focusField(field: HTMLTextAreaElement) {
  field.focus();
  field.dispatchEvent(new FocusEvent('focus'));
}

function blurField(field: HTMLTextAreaElement) {
  field.blur();
  field.dispatchEvent(new FocusEvent('blur'));
}

function historyBox(onHistory: (signal: AbortSignal) => Promise<string[]>, completions: {
  onMention?: (query: string) => Promise<Mention[]>;
  onCommands?: () => Promise<Command[]>;
} = {}) {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;bottom:0;width:720px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const sent: string[] = [];
  const drafts: Draft[] = [];
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Write',
    draftKey: 'history-regression', onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    ...completions, onHistory, onSend: (draft) => { sent.push(draft.text); drafts.push(draft); },
  }), root);
  const field = root.querySelector('textarea')!;
  return {
    root, field, sent, drafts,
    settle: () => new Promise((resolve) => setTimeout(resolve, 100)),
    type: (text: string) => {
      field.value = text;
      field.setSelectionRange(text.length, text.length);
      field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    },
    press: (key: string, options: KeyboardEventInit = {}) => {
      const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...options });
      field.dispatchEvent(event);
      return event;
    },
    close: () => { dispose(); root.remove(); localStorage.removeItem('faber.unsent.history-regression'); },
  };
}

check('prompt-history: drop, attach, paste and attachment removal commit the edited message as the draft', async () => {
  for (const action of ['drop-text', 'drop-file', 'attach-text', 'paste-text', 'paste-image', 'remove-image', 'remove-file', 'drop-while-typing']) {
    const box = historyBox(async () => ['recalled prompt']);
    const deliver = (kind: 'drop' | 'paste' | 'change', file: File) => {
      const transfer = new DataTransfer();
      transfer.items.add(file);
      const event = new Event(kind, { bubbles: true, cancelable: true });
      if (kind === 'change') {
        const input = box.root.querySelector<HTMLInputElement>('input[type="file"]')!;
        input.files = transfer.files;
        input.dispatchEvent(event);
      } else {
        Object.defineProperty(event, kind === 'drop' ? 'dataTransfer' : 'clipboardData', { value: transfer });
        (kind === 'drop' ? box.root.querySelector('form')! : box.field).dispatchEvent(event);
      }
    };
    try {
      await box.settle();
      focusField(box.field);
      box.type('original unfinished draft');
      if (action === 'remove-image') {
        deliver('drop', new File([new Uint8Array([137, 80, 78, 71])], 'image.png', { type: 'image/png' }));
        await box.settle();
      } else if (action === 'remove-file') {
        deliver('drop', new File(['binary payload'], 'archive.bin', { type: 'application/octet-stream' }));
        await box.settle();
      }
      box.press('ArrowUp'); await box.settle();
      equal(box.field.value, 'recalled prompt', action);
      if (action === 'paste-text') {
        box.field.value += ' pasted text';
        box.field.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: ' pasted text' }));
      } else if (action === 'paste-image') {
        deliver('paste', new File([new Uint8Array([137, 80, 78, 71])], 'image.png', { type: 'image/png' }));
      } else if (action.startsWith('remove-')) {
        const key = action === 'remove-image' ? 'composer.removeImage' : 'composer.removeAttachments';
        const button = [...box.root.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.getAttribute('aria-label') === t(key))!;
        ok(button, `the attachment exists before ${action}`);
        const down = new MouseEvent('mousedown', { button: 0, bubbles: true, cancelable: true });
        button.dispatchEvent(down);
        ok(down.defaultPrevented, 'the editing control keeps focus until it changes the current content');
        button.click();
      } else {
        const file = action === 'drop-file'
          ? new File(['binary payload'], 'archive.bin', { type: 'application/octet-stream' })
          : new File(['file contents that must survive'], 'notes.txt', { type: 'text/plain' });
        deliver(action === 'attach-text' ? 'change' : 'drop', file);
        if (action === 'drop-while-typing') box.type('typed while the file was loading');
      }
      await box.settle();
      const edited = box.field.value;
      if (['drop-text', 'attach-text', 'drop-while-typing'].includes(action)) {
        ok(edited.includes('file contents that must survive'), action);
      }
      if (action === 'paste-text') equal(edited, 'recalled prompt pasted text');
      ok(edited.startsWith(action === 'drop-while-typing' ? 'typed while the file was loading' : 'recalled prompt'), action);
      box.press('Escape'); box.press('ArrowDown');
      equal(box.field.value, edited, `${action}: neither Escape nor Down may restore the old snapshot`);
      equal(localStorage.getItem('faber.unsent.history-regression'), edited, `${action}: the edited text is saved`);
      box.press('Enter');
      const sent = box.drafts.at(-1)!;
      equal(sent.text, edited);
      equal(sent.images.length, action === 'paste-image' ? 1 : 0, action);
      equal(sent.files.length, action === 'drop-file' ? 1 : 0, action);
    } finally { box.close(); }
  }
});

check('prompt-history: pending completion requests still own Up but finished empty file and command lists do not', async () => {
  let finish!: () => void;
  let waiting: Promise<void> | undefined;
  let reads = 0;
  const box = historyBox(async () => { reads++; return ['recalled prompt']; }, {
    onMention: async (query) => { await waiting; return query === 'known' ? [{ path: 'known.txt', dir: false }] : []; },
    onCommands: async () => { await waiting; return [{ vendor: 'claude', name: 'known', skill: false }]; },
  });
  try {
    await box.settle();
    focusField(box.field);
    for (const prefix of ['@', '/']) {
      waiting = new Promise<void>((resolve) => { finish = resolve; });
      const typed = `${prefix}not-a-match`;
      box.type(typed); await box.settle();
      const before = reads;
      box.press('ArrowUp'); await box.settle();
      equal(reads, before, 'a pending request retains Up even before the list arrives');
      equal(box.field.value, typed);
      finish(); waiting = undefined; await box.settle();
      equal(box.root.querySelector('[role="listbox"]'), null, 'the request finished with no visible matches');
      ok(box.press('ArrowUp').defaultPrevented, 'a finished empty list releases Up to history');
      await box.settle();
      equal(box.field.value, 'recalled prompt');
      box.press('Escape');
      equal(box.field.value, typed, 'the unmatched token remains the original draft');
      box.type(`${prefix}known`); await box.settle();
      ok(box.root.querySelector('[role="listbox"]'), 'a populated list still owns its arrows');
      box.press('ArrowUp'); await box.settle();
      equal(box.field.value, `${prefix}known`);
      box.press('Escape');
    }
  } finally { box.close(); }
});

check('prompt-history: Enter and Send wait for a delayed recall and blur cancels the read', async () => {
  let finish!: (messages: string[]) => void;
  let signal!: AbortSignal;
  const box = historyBox((readSignal) => {
    signal = readSignal;
    return new Promise((resolve) => { finish = resolve; });
  });
  try {
    await box.settle();
    focusField(box.field);
    box.type('unfinished draft');
    box.press('ArrowUp');
    ok(box.root.querySelector<HTMLButtonElement>('button[type="submit"]')!.disabled, 'Send waits while the read is pending');
    box.press('Enter');
    box.root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    deepEqual(box.sent, [], 'neither Enter nor form submit sends the unfinished draft');
    finish(['recalled prompt']); await box.settle();
    equal(box.field.value, 'recalled prompt');
    box.press('Enter');
    deepEqual(box.sent, ['recalled prompt'], 'a separate Enter after recall sends only the recalled text');

    box.type('search while recall loads');
    box.press('ArrowUp');
    box.press('r', { ctrlKey: true });
    finish(['picked from search']); await box.settle();
    box.press('Enter'); await box.settle();
    equal(box.field.value, 'picked from search');
    box.press('Enter');
    deepEqual(box.sent, ['recalled prompt', 'picked from search'], 'switching a pending recall to search does not leave sending blocked');

    box.type('another draft');
    box.press('ArrowUp');
    blurField(box.field);
    ok(signal.aborted, 'leaving the box aborts the engine request');
    focusField(box.field);
    finish(['late text']); await box.settle();
    equal(box.field.value, 'another draft', 'the cancelled answer cannot change the restored draft');
  } finally { box.close(); }
});

check('prompt-history: visiting another control while browsing preserves the saved draft and its caret', async () => {
  const box = historyBox(async () => ['recalled prompt']);
  try {
    await box.settle();
    focusField(box.field);
    box.type('original unfinished draft');
    box.field.setSelectionRange(4, 4);
    box.press('ArrowUp'); await box.settle();
    equal(box.field.value, 'recalled prompt');
    box.press('Tab');
    blurField(box.field);
    box.root.querySelector<HTMLButtonElement>('.iconbtn')!.focus();
    equal(box.field.value, 'original unfinished draft', 'browsing restores the draft when focus leaves');
    equal(localStorage.getItem('faber.unsent.history-regression'), 'original unfinished draft');
    equal(box.field.selectionStart, 4, 'restoration keeps the original caret');
    focusField(box.field);
    box.press('ArrowDown'); box.press('Escape');
    equal(box.field.value, 'original unfinished draft');
    box.press('ArrowUp'); await box.settle();
    const send = box.root.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    const down = new MouseEvent('mousedown', { button: 0, bubbles: true, cancelable: true });
    send.dispatchEvent(down);
    ok(down.defaultPrevented, 'clicking Send keeps the recalled message focused until it is sent');
    send.click();
    deepEqual(box.sent, ['recalled prompt'], 'Send still sends the recalled message');
  } finally { box.close(); }
});

check('prompt-history: Up stays in a visually wrapped paragraph at 720 px and recalls only on its first visual line', async () => {
  let reads = 0;
  const box = historyBox(async () => { reads++; return ['recalled prompt']; });
  try {
    await box.settle();
    focusField(box.field);
    equal(box.root.clientWidth, 720);
    const paragraph = 'An ordinary long prompt that wraps across the message box. '.repeat(12);
    box.type(paragraph); await box.settle();
    const style = getComputedStyle(box.field);
    const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
    ok(box.field.scrollHeight > parseFloat(style.lineHeight) * 2 + padding, 'the browser laid out several visual lines without newlines');
    ok(!box.press('ArrowUp').defaultPrevented, 'Up on a wrapped line keeps native caret navigation');
    await box.settle();
    equal(reads, 0, 'later visual lines do not read history');
    equal(box.field.value, paragraph);
    box.field.setSelectionRange(1, 1);
    ok(box.press('ArrowUp').defaultPrevented, 'Up on the first visual line recalls');
    await box.settle();
    equal(box.field.value, 'recalled prompt');
    box.press('Escape');
    equal(box.field.value, paragraph);
    equal(box.field.selectionStart, 1);
  } finally { box.close(); }
});

check('prompt-history: App scopes history to the open area and project, including a new conversation', async () => {
  const before = { promptHistory: api.promptHistory, state: api.state, open: api.open, setClock: api.setClock,
    setShellTexts: api.setShellTexts, asides: api.asides };
  const storage = new Map(['faber.place', 'faber.unsent.history-conversation', 'faber.unsent.new-chat-',
    'faber.unsent.new-project-history-project'].map((key) => [key, localStorage.getItem(key)]));
  const calls: [string | undefined, Area, string | undefined][] = [];
  const signals: AbortSignal[] = [];
  api.promptHistory = async (id, area, project, signal) => {
    ok(signal, 'App passes cancellation through to the engine request');
    signals.push(signal);
    calls.push([id, area, project]);
    return ['saved prompt'];
  };
  api.setClock = async () => {}; api.setShellTexts = async () => {}; api.asides = async () => [];
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  try {
    for (const area of ['chat', 'project'] as const) {
      for (const opened of [true, false]) {
        const projectId = area === 'project' ? 'history-project' : undefined;
        const conversation = { id: 'history-conversation', title: 'Earlier work', area, projectId, createdAt: 1, updatedAt: 1,
          sessions: {}, messages: [{ id: 'answer', role: 'assistant', text: 'Done', at: 1, status: 'done' }],
        } as Conversation;
        const initial = {
          conversations: [{ id: conversation.id, title: conversation.title, area, projectId, updatedAt: 1 }],
          projects: [{ id: 'history-project', name: 'History project', path: 'C:/history-project' }],
          vendors: ['claude'], models: [], settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
          running: { revision: 0, items: [] }, quotas: [], usage: [], version: 'test',
          look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
            appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14,
              density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
        } as AppState;
        api.state = async () => initial; api.open = async () => conversation;
        localStorage.removeItem(`faber.unsent.${opened ? conversation.id : `new-${area}-${projectId ?? ''}`}`);
        localStorage.setItem('faber.place', JSON.stringify({ area, id: opened ? conversation.id : undefined, projectId }));
        const root = document.createElement('div');
        document.body.append(root);
        const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
        try {
          await settle();
          const field = root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
          ok(field, 'both areas have the shared message box');
          focusField(field);
          field.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true }));
          await settle();
          deepEqual(calls.at(-1), [opened ? conversation.id : undefined, area, projectId]);
          equal(field.value, 'saved prompt');
          field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
          blurField(field);
          ok(signals.at(-1)!.aborted, 'leaving either area cancels its request');
        } finally { dispose(); root.remove(); }
      }
    }
  } finally {
    Object.assign(api, before);
    for (const [key, value] of storage) {
      if (value === null) localStorage.removeItem(key); else localStorage.setItem(key, value);
    }
  }
});

check('prompt-history: Up, Down and search keep the draft and share the completion list', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;bottom:0;width:600px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  let reads = 0;
  let sent = '';
  let fail = false;
  let finish: ((messages: string[]) => void) | undefined;
  const messages = ['newest question', 'older café question', 'oldest question'];
  const draftKey = 'prompt-history-check';
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude', 'codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Write', draftKey,
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onSend: (draft) => { sent = draft.text; },
    onHistory: async () => {
      reads++;
      if (fail) throw new Error('unreadable history');
      if (reads > 1) return new Promise<string[]>((resolve) => { finish = resolve; });
      return messages;
    },
    onMention: async () => [{ path: 'README.md', dir: false }],
    onCommands: async () => [{ vendor: 'claude', name: 'review', skill: false }],
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  const field = () => root.querySelector('textarea')!;
  const type = (text: string) => {
    field().value = text;
    field().setSelectionRange(text.length, text.length);
    field().dispatchEvent(new InputEvent('input', { bubbles: true }));
  };
  const press = (key: string, options: KeyboardEventInit = {}) => field().dispatchEvent(new KeyboardEvent('keydown', {
    key, bubbles: true, cancelable: true, ...options,
  }));
  const shown = () => [...root.querySelectorAll('.mentions [role="option"] span:first-of-type')].map((span) => span.textContent);
  try {
    await settle();
    focusField(field());
    type('my untouched draft');
    press('ArrowUp'); await settle();
    equal(reads, 1, 'Up reads history after the box receives focus');
    equal(field().value, messages[0]);
    press('ArrowUp'); await settle();
    equal(field().value, messages[1]);
    press('ArrowDown'); await settle();
    equal(field().value, messages[0]);
    press('ArrowDown'); await settle();
    equal(field().value, 'my untouched draft', 'Down past newest returns the exact draft');
    equal(field().selectionStart, 'my untouched draft'.length, 'its caret comes back too');
    press('ArrowUp'); await settle();
    press('Escape');
    equal(field().value, 'my untouched draft');
    press('ArrowUp'); await settle();
    type('edited recalled question');
    press('ArrowDown'); await settle();
    equal(field().value, 'edited recalled question', 'editing ends browsing');
    press('ArrowUp'); await settle();
    press('Escape');
    equal(field().value, 'edited recalled question', 'the edit is the new draft');

    type('line one\nline two');
    press('ArrowUp'); await settle();
    equal(field().value, 'line one\nline two', 'Up on a later line edits normally');
    field().setSelectionRange(2, 2);
    press('ArrowUp'); await settle();
    equal(field().value, messages[0], 'Up on the first line recalls');
    press('Escape');
    equal(field().selectionStart, 2);
    press('ArrowUp', { isComposing: true }); await settle();
    equal(field().value, 'line one\nline two', 'composition never recalls');

    type('@'); await settle();
    press('ArrowUp'); await settle();
    equal(field().value, '@', 'Up belongs to an open @ list');
    press('Escape');
    type('/'); await settle();
    press('ArrowUp'); await settle();
    equal(field().value, '/', 'Up belongs to an open / list');
    press('Escape');

    type('search draft');
    press('r', { ctrlKey: true }); await settle();
    deepEqual(shown(), messages);
    equal(root.querySelectorAll('[role="listbox"]').length, 1, 'history reuses the only list');
    equal(root.querySelector('[role="listbox"]')?.getAttribute('aria-label'), t('composer.history'));
    type('CAFÉ'); await settle();
    ok(root.querySelector<HTMLButtonElement>('button[type="submit"]')!.disabled, 'search cannot send through the button');
    deepEqual(shown(), [messages[1]], 'typing filters without asking again');
    equal(localStorage.getItem(`faber.unsent.${draftKey}`), 'search draft', 'the query never replaces the saved draft');
    press('Enter'); await settle();
    equal(field().value, messages[1]);
    equal(sent, '', 'Enter chooses without sending');
    press('Escape');
    equal(field().value, 'search draft');
    equal(reads, 1, 'arrows and searches share one read during focus');

    press('r', { ctrlKey: true }); await settle();
    type('no match'); await settle();
    ok(root.textContent?.includes(t('composer.historyEmpty')));
    press('Enter');
    equal(sent, '', 'no match cannot send the search');
    press('Escape');
    equal(field().value, 'search draft');
    press('r', { ctrlKey: true }); await settle();
    blurField(field()); await settle();
    equal(field().value, 'search draft', 'leaving search restores the draft');
    focusField(field());
    press('ArrowUp');
    press('ArrowUp');
    await settle();
    equal(reads, 2, 'a new focus reads again');
    finish!(messages); await settle();
    equal(field().value, messages[1], 'rapid arrows while reading are preserved');
    press('Escape');

    blurField(field()); focusField(field());
    press('ArrowUp'); await settle();
    blurField(field()); focusField(field());
    type('new focus draft');
    finish!(messages); await settle();
    equal(field().value, 'new focus draft', 'a stale read cannot overwrite a later focus');
    blurField(field()); focusField(field());
    fail = true;
    press('r', { ctrlKey: true }); await settle();
    ok(root.textContent?.includes(t('composer.historyFailed')), 'a read failure is explained');
    press('Escape');
    equal(field().value, 'new focus draft', 'a failure loses no draft');
  } finally {
    dispose(); root.remove(); localStorage.removeItem(`faber.unsent.${draftKey}`);
  }
});
