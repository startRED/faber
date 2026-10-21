import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { App } from '../../ui/App.tsx';
import { Composer } from '../../ui/Composer.tsx';
import { api, type AppEvent, type AppState, type Choice, type Command, type Conversation, type Vendor } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

check('slash: App uses each AI\'s own or last list, refreshes on announcements and preserves lists during streaming', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const before = { open: api.open, state: api.state, send: api.send, commands: api.commands, warm: api.warm, routeDraft: api.routeDraft,
    setClock: api.setClock, setShellTexts: api.setShellTexts, asides: api.asides, mentions: api.mentions };
  let conversation: Conversation = {
    id: 'slash-app-switch', title: 'Review notes', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {},
    choice: { vendor: 'claude', model: 'sonnet' },
    messages: [{ id: 'answer', role: 'assistant', text: 'Reviewed', at: 1, status: 'done', vendor: 'claude', model: 'sonnet' }],
  };
  const storage = new Map(['faber.place', `faber.choice.${conversation.id}`].map((key) => [key, localStorage.getItem(key)]));
  const initial = {
    conversations: [{ id: conversation.id, title: conversation.title, area: conversation.area, updatedAt: 1 }], projects: [],
    vendors: ['claude', 'codex'], models: [{ id: 'test-model', label: 'Test model', isDefault: true }],
    settings: { setup: 'done', tutorialDone: true, language: 'en', webSearch: 'off', appearance: { base: 'sala' } },
    running: { revision: 0, items: [] }, quotas: [], usage: [], version: 'test',
    look: {
      vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14,
        density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
    },
  } as AppState;
  const asked: [string | undefined, Vendor | undefined][] = [];
  let mentionsAsked = 0;
  let completionWait: Promise<void> | undefined;
  const sent: { text: string; choice: Choice }[] = [];
  const own: Partial<Record<Vendor, Command[]>> = { claude: [{ vendor: 'claude', name: 'context', skill: false }] };
  const latest: Record<Vendor, Command[]> = {
    claude: [{ vendor: 'claude', name: 'review', skill: false }],
    codex: [{ vendor: 'codex', name: 'review-notes', skill: true }],
  };
  api.open = async () => conversation;
  api.state = async () => initial;
  api.send = async (request) => { sent.push({ text: request.text, choice: request.choice }); return conversation.id; };
  api.commands = async (id, vendor) => {
    asked.push([id, vendor]);
    await completionWait;
    return (vendor ? [vendor] : ['claude', 'codex'] as const).flatMap((ai) =>
      (id === conversation.id ? own[ai] : undefined) ?? latest[ai]);
  };
  api.mentions = async () => {
    mentionsAsked++;
    await completionWait;
    return [{ path: 'notes.md', dir: false }];
  };
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.asides = async () => [];
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: conversation.id }));
  localStorage.removeItem(`faber.choice.${conversation.id}`);
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener) => { if (listener) hear = listener; } } }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  const field = () => root.querySelector<HTMLTextAreaElement>('.composer textarea')!;
  const select = (model: string) => {
    const picker = root.querySelector<HTMLSelectElement>('.composer .choose select')!;
    picker.value = [...picker.options].find((option) => option.textContent?.includes(model))!.value;
    picker.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const type = (text: string) => {
    field().value = text;
    field().setSelectionRange(text.length, text.length);
    field().dispatchEvent(new InputEvent('input', { bubbles: true }));
  };
  const enter = () => field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  const shown = () => [...root.querySelectorAll('.mentions [role="option"] span:first-of-type')].map((span) => span.textContent);
  // The agent's own commands, without Faber's quick question, which comes first whenever it is available.
  const cli = () => shown().filter((name) => name !== '/btw' && name !== '/recap');
  const down = () => field().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
  try {
    await settle();
    type('/');
    await settle();
    deepEqual(cli(), ['/context'], 'Claude uses this conversation\'s own list before its last list');
    ok(shown().includes('/btw'), 'the quick question is offered in an open conversation, as its head button is');
    select('Test-model');
    await settle();
    deepEqual(asked.at(-1), [conversation.id, 'codex'], 'the latest Claude answer cannot override the current choice');
    deepEqual(cli(), ['$review-notes'], 'the other AI never leaks into the list');
    down();
    down();
    enter();
    await settle();
    equal(field().value, '$review-notes ');
    equal(sent.length, 0, 'picking does not send');
    enter();
    await settle();
    equal(sent[0]?.text, '$review-notes ');
    equal(sent[0]?.choice.vendor, 'codex', 'the command and send agree');

    const answer = { ...conversation.messages[0]!, vendor: 'codex' as const, model: 'test-model' };
    conversation = { ...conversation, choice: { vendor: 'codex', model: 'test-model' }, messages: [answer] };
    hear({ type: 'message', conversationId: conversation.id, message: answer });
    await settle();
    type('/');
    await settle();
    deepEqual(cli(), ['$review-notes']);
    select('Sonnet');
    await settle();
    deepEqual(asked.at(-1), [conversation.id, 'claude'], 'switching refreshes an already open token too');
    deepEqual(cli(), ['/context'], 'a Codex answer cannot override the new Claude choice');
    down();
    down();
    enter();
    await settle();
    equal(field().value, '/context ');
    equal(sent.length, 1);
    enter();
    await settle();
    equal(sent[1]?.text, '/context ');
    equal(sent[1]?.choice.vendor, 'claude');

    own.claude = [];
    type('/');
    await settle();
    deepEqual(cli(), [], 'an empty own list overrides an older fallback');
    const queries = asked.length;
    own.claude = [{ vendor: 'claude', name: 'context', skill: false }];
    hear({ type: 'commands', conversationId: conversation.id, vendor: 'claude' });
    await settle();
    equal(asked.length, queries + 1, 'the announcement asks once without a click, focus or edit');
    deepEqual(cli(), ['/context']);
    equal(field().value, '/');

    type('');
    await settle();
    select('Test-model');
    type('/');
    await settle();
    const fallbackQueries = asked.length;
    latest.codex = [{ vendor: 'codex', name: 'tidy', skill: true }];
    hear({ type: 'commands', conversationId: 'another-conversation', vendor: 'codex' });
    await settle();
    equal(asked.length, fallbackQueries + 1, 'a matching AI announcement from another conversation updates its fallback');
    deepEqual(cli(), ['$tidy']);
    hear({ type: 'commands', conversationId: 'another-conversation', vendor: 'claude' });
    await settle();
    equal(asked.length, fallbackQueries + 1, 'an unrelated AI announcement does not query');

    type('');
    select('Sonnet');
    let updates = 0;
    const stream = () => {
      const message = { ...conversation.messages[0]!, status: 'streaming' as const, text: `Answer part ${++updates}` };
      conversation = { ...conversation, messages: [message] };
      hear({ type: 'message', conversationId: conversation.id, message });
    };
    stream();
    await settle();
    field().focus();
    const sentBeforeStreaming = sent.length;
    for (const token of ['/', '@']) {
      for (const stage of ['open', 'scheduled', 'answer']) {
        type('');
        let finish: (() => void) | undefined;
        completionWait = stage === 'answer' ? new Promise<void>((resolve) => { finish = resolve; }) : undefined;
        const queries = () => token === '/' ? asked.length : mentionsAsked;
        const beforeQueries = queries();
        type(token);
        if (stage !== 'scheduled') await settle();
        const first = root.querySelector('.mentions [role="option"]');
        if (stage === 'open') ok(first, 'the list starts open');
        if (stage === 'answer') {
          equal(queries(), beforeQueries + 1, 'the request is already in flight');
          deepEqual(shown(), [], 'the answer is still pending');
        }
        for (let i = 0; i < 15; i++) {
          stream();
          if (stage === 'open') equal(root.querySelector('.mentions [role="option"]'), first, 'streaming keeps the open rows');
          if (i === 3) finish?.();
          await new Promise((resolve) => setTimeout(resolve, 20));
          if (i >= 6) deepEqual(shown(), token === '/' ? ['/btw', '/recap', '/context'] : ['notes.md'], `${token} ${stage} completes during streaming`);
        }
        equal(queries(), beforeQueries + 1, 'streaming never restarts a completion query');
        equal(document.activeElement, field(), 'the field stays focused');
        enter();
        equal(field().value, token === '/' ? '/btw ' : '@notes.md ', 'Enter inserts while the answer is streaming');
        equal(sent.length, sentBeforeStreaming, 'a streaming completion never sends');
      }
    }
  } finally {
    dispose();
    root.remove();
    Object.assign(api, before);
    for (const [key, value] of storage) {
      if (value === null) localStorage.removeItem(key); else localStorage.setItem(key, value);
    }
  }
});

check('slash: announcements refresh empty and pending queries, ignore stale answers and leave closed lists closed', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  let commands: Command[] = [];
  let asked = 0;
  const [version, setVersion] = createSignal(0);
  let delayed: ((commands: Command[]) => void) | undefined;
  let delay = false;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'claude' }, busy: false, placeholder: 'Write',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
    get commandsVersion() { return version(); },
    onCommands: async () => {
      asked++;
      if (delay) return new Promise<Command[]>((resolve) => { delayed = resolve; });
      return commands;
    },
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  const shown = () => [...root.querySelectorAll('.mentions [role="option"] span:first-of-type')].map((span) => span.textContent);
  try {
    await settle();
    const field = root.querySelector('textarea')!;
    field.value = '/';
    field.setSelectionRange(1, 1);
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle();
    equal(asked, 1);
    deepEqual(shown(), [], 'the first query completes before the announcement');
    commands = [{ vendor: 'claude', name: 'context', skill: false }];
    setVersion((n) => n + 1);
    await settle();
    equal(asked, 2, 'the announcement alone asks again');
    deepEqual(shown(), ['/context']);
    equal(field.value, '/', 'no typing is needed to refresh');

    delay = true;
    setVersion((n) => n + 1);
    await settle();
    equal(asked, 3);
    delay = false;
    commands = [{ vendor: 'claude', name: 'review', skill: false }];
    setVersion((n) => n + 1);
    await settle();
    equal(asked, 4, 'an announcement replaces a pending query too');
    delayed!([{ vendor: 'claude', name: 'stale', skill: false }]);
    await settle();
    deepEqual(shown(), ['/review']);
    field.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await settle();
    equal(asked, 4, 'a click at the same caret needs no extra refresh');
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    setVersion((n) => n + 1);
    await settle();
    equal(asked, 4, 'announcements never reopen a dismissed list');
    deepEqual(shown(), []);

    commands = [];
    field.value = '/';
    field.setSelectionRange(1, 1);
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle();
    equal(asked, 5);
    deepEqual(shown(), []);
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    commands = [{ vendor: 'claude', name: 'context', skill: false }];
    setVersion((n) => n + 1);
    await settle();
    equal(asked, 5, 'Escape also dismisses a query that completed empty');
    deepEqual(shown(), []);
  } finally {
    dispose();
    root.remove();
  }
});

check('slash: the shared list filters, inserts without sending, and closes with Esc or a moved caret', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;bottom:0;width:600px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.dataset.motion = 'none';
  document.body.append(root);
  const commands: Command[] = [
    { vendor: 'claude', name: 'review-notes', skill: true, note: 'Review the notes' },
    { vendor: 'claude', name: 'context', skill: false },
    { vendor: 'claude', name: 'view', skill: true },
  ];
  let sent = '';
  let slow = false;
  let failed = false;
  let escaped = 0;
  const onWindowEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') escaped++; };
  window.addEventListener('keydown', onWindowEscape);
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'claude' }, busy: false, placeholder: 'Write',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onSend: (draft) => { sent = draft.text; },
    onCommands: async () => {
      if (slow) await new Promise((resolve) => setTimeout(resolve, 300));
      if (failed) throw new Error('No command list');
      return commands;
    },
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  const field = () => root.querySelector('textarea')!;
  const type = (text: string) => {
    field().value = text;
    field().setSelectionRange(text.length, text.length);
    field().dispatchEvent(new InputEvent('input', { bubbles: true }));
  };
  const press = (key: string) => field().dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
  const shown = () => [...root.querySelectorAll('.mentions [role="option"]')].map((li) => li.querySelector('span')?.textContent);
  try {
    await settle();
    type('/');
    await settle();
    deepEqual(shown(), ['/review-notes', '/context', '/view']);
    equal(root.querySelector('[role="listbox"]')?.getAttribute('aria-label'), t('composer.commands'));
    equal(field().getAttribute('aria-controls'), 'composer-mentions', 'the same list is wired to the field');
    equal(field().getAttribute('aria-activedescendant'), 'mention-0');
    equal(root.querySelector('.mention-note')?.textContent, 'Review the notes');
    equal(root.querySelector('.mention-vendor'), null, 'one AI needs no label');
    type('/VI');
    await settle();
    deepEqual(shown(), ['/view', '/review-notes'], 'prefixes come before matches inside a name, ignoring case');
    press('Enter');
    await settle();
    equal(field().value, '/view ');
    equal(sent, '', 'picking sends nothing');
    equal(root.querySelector('.mentions'), null);

    type('/con');
    await settle();
    press('Escape');
    await settle();
    equal(root.querySelector('.mentions'), null);
    equal(field().value, '/con');
    equal(escaped, 0, 'Esc does not close another surface');
    press('Enter');
    equal(sent, '/con', 'with the list closed Enter sends the text');

    type('explain /context');
    await settle();
    equal(root.querySelector('.mentions'), null, 'a slash inside the message opens nothing');
    type('/context args');
    await settle();
    equal(root.querySelector('.mentions'), null, 'arguments finish the command token');
    type('/');
    await settle();
    field().setSelectionRange(0, 0);
    field().dispatchEvent(new KeyboardEvent('keyup', { key: 'Home', bubbles: true }));
    await settle();
    equal(root.querySelector('.mentions'), null, 'the caret must follow the token');

    sent = '';
    slow = true;
    type('/');
    await settle();
    press('Enter');
    equal(sent, '', 'pending Enter never sends');
    equal(field().value, '/');
    await new Promise((resolve) => setTimeout(resolve, 350));
    type('/con');
    press('Enter');
    equal(field().value, '/con', 'pending Enter cannot insert an older row');
    press('Escape');
    await new Promise((resolve) => setTimeout(resolve, 450));
    equal(root.querySelector('.mentions'), null, 'a late answer cannot reopen a dismissed list');
    slow = false;
    failed = true;
    type('/bad');
    await settle();
    press('Enter');
    equal(sent, '/bad', 'a failed list releases Enter and leaves the typed command usable');

    failed = false;
    commands.push(...Array.from({ length: 60 }, (_, i): Command => ({ vendor: 'claude', name: `task-${i}`, skill: true })));
    type('/');
    await settle();
    equal(shown().length, 50, 'the shared list has at most 50 rows');
  } finally {
    window.removeEventListener('keydown', onWindowEscape);
    dispose();
    root.remove();
  }
});

check('slash: a skill uses its agent prefix and Auto moves the existing model choice to that agent', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [choice, setChoice] = createSignal<Choice>({ vendor: 'auto' });
  let sent = false;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude', 'codex'], busy: false, placeholder: 'Write',
    options: [
      { label: 'Auto', choice: { vendor: 'auto' } },
      { label: 'Sonnet', choice: { vendor: 'claude', model: 'sonnet' } },
      { label: 'Agent model', choice: { vendor: 'codex', model: 'test-model' } },
    ],
    get choice() { return choice(); },
    onChoice: setChoice, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => { sent = true; },
    onCommands: async () => [
      { vendor: 'claude', name: 'review', skill: false },
      { vendor: 'codex', name: 'review-notes', skill: true, note: 'Check the notes' },
    ],
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  try {
    await settle();
    const field = root.querySelector('textarea')!;
    field.value = '/';
    field.setSelectionRange(1, 1);
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle();
    const labels = [...root.querySelectorAll('.mention-vendor')].map((el) => el.textContent);
    deepEqual(labels, [t('composer.commandAgent', { name: 'Claude' }), t('composer.commandAgent', { name: 'Codex' })]);
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await settle();
    equal(field.value, '$review-notes ', 'the skill is written as the agent expects');
    equal(choice().vendor, 'codex', 'the existing onChoice receives the selected agent');
    equal(choice().model, 'test-model', 'the model picker can show that choice');
    equal(root.querySelector('select')?.value, '2');
    ok(!sent, 'a skill pick never sends');
  } finally {
    dispose();
    root.remove();
  }
});

check('slash: the quick question comes first only where available and inserts without sending or changing Auto', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [available, setAvailable] = createSignal(false);
  let failed = false;
  let slow = false;
  let sent = 0;
  let choices = 0;
  let recaps = 0;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude', 'codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Write',
    get canAside() { return available(); },
    onRecap: () => { recaps++; },
    onChoice: () => { choices++; }, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onSend: () => { sent++; },
    onCommands: async () => {
      if (slow) await new Promise((resolve) => setTimeout(resolve, 400));
      if (failed) throw new Error('No commands');
      return [
        { vendor: 'claude', name: 'context', skill: false },
        { vendor: 'codex', name: 'review', skill: true },
      ];
    },
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  const field = () => root.querySelector('textarea')!;
  const type = (text: string) => {
    field().value = text;
    field().setSelectionRange(text.length, text.length);
    field().dispatchEvent(new InputEvent('input', { bubbles: true }));
  };
  const rows = () => [...root.querySelectorAll<HTMLElement>('.mentions [role="option"]')];
  const shown = () => rows().map((li) => li.querySelector('span')?.textContent);
  try {
    await settle();
    type('/');
    await settle();
    deepEqual(shown(), ['/context', '$review'], 'a new conversation offers no quick question');
    setAvailable(true);
    await settle();
    deepEqual(shown(), ['/btw', '/recap', '/context', '$review']);
    equal(rows()[0]!.querySelector('.mention-note')?.textContent, t('aside.title'));
    equal(rows()[0]!.querySelector('.mention-vendor'), null, 'Faber owns the action');
    rows()[0]!.dispatchEvent(new MouseEvent('mousedown', { button: 2, bubbles: true, cancelable: true }));
    equal(field().value, '/', 'only the main button picks');
    rows()[0]!.dispatchEvent(new MouseEvent('mousedown', { button: 0, bubbles: true, cancelable: true }));
    await settle();
    equal(field().value, '/btw ');
    equal(sent, 0);
    equal(choices, 0, 'the quick question keeps the existing choice');

    type('/RECAP');
    await settle();
    deepEqual(shown(), ['/recap']);
    equal(rows()[0]!.querySelector('.mention-note')?.textContent, t('aside.recap'));
    field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await settle();
    equal(recaps, 1, 'choosing the recap asks at once');
    equal(field().value, '', 'the recap inserts nothing');
    equal(sent, 0, 'nothing is sent to the main conversation');
    equal(choices, 0, 'the recap keeps Auto');

    // The / list opens only for a box holding the command alone, so a recap never meets a draft here; the mouse asks too.
    type('/recap');
    await settle();
    rows()[0]!.dispatchEvent(new MouseEvent('mousedown', { button: 0, bubbles: true, cancelable: true }));
    await settle();
    equal(recaps, 2, 'choosing the recap with the mouse asks too');
    equal(field().value, '');

    type('/BT');
    await settle();
    deepEqual(shown(), ['/btw'], 'the action uses the same case-insensitive filtering');
    field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await settle();
    equal(field().value, '/btw ', 'Enter inserts too');
    equal(sent, 0);
    type('/context');
    await settle();
    deepEqual(shown(), ['/context'], 'an unmatched action stays out');

    failed = true;
    type('/');
    await settle();
    deepEqual(shown(), ['/btw', '/recap'], 'a failed agent list cannot hide Faber\'s actions');
    setAvailable(false);
    await settle();
    equal(root.querySelector('.mentions'), null, 'availability changes refresh the same token');

    failed = false;
    slow = true;
    setAvailable(true);
    await settle();
    setAvailable(false);
    await new Promise((resolve) => setTimeout(resolve, 550));
    deepEqual(shown(), ['/context', '$review'], 'an older available answer never brings the action back');
  } finally {
    dispose();
    root.remove();
  }
});

check('slash: pending interactions, callbacks and cleanup are shared with mentions', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const asked: string[] = [];
  const [commands, setCommands] = createSignal<() => Promise<Command[]>>(() => {
    asked.push('original');
    return Promise.resolve([{ vendor: 'claude', name: 'context', skill: false }]);
  });
  let sent = 0;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'claude' }, busy: false, placeholder: 'Write',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => { sent++; },
    get onCommands() { return commands(); },
    onMention: async () => [{ path: 'src/', dir: true }],
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
  const field = root.querySelector('textarea')!;
  const type = (text: string) => {
    field.value = text;
    field.setSelectionRange(text.length, text.length);
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
  };
  const press = (key: string) => {
    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
    field.dispatchEvent(event);
    return event.defaultPrevented;
  };
  try {
    await settle();
    type('/');
    equal(asked.length, 0, 'the query waits for the 60 ms pause');
    // Tab is not checked here: the app's own focus trap, left by other checks on the window, may take it.
    ok(!press('ArrowDown'), 'arrows move normally without a visible list');
    ok(press('Enter'), 'Enter waits for the current answer');
    equal(sent, 0);
    setCommands(() => async () => {
      asked.push('replacement');
      return [];
    });
    await settle();
    deepEqual(asked, ['original'], 'the scheduled query keeps its callback');
    field.setSelectionRange(0, 0);
    field.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    equal(root.querySelector('.mentions'), null, 'a click follows the caret');
    field.setSelectionRange(1, 1);
    field.dispatchEvent(new KeyboardEvent('keyup', { key: 'End', bubbles: true }));
    await settle();
    deepEqual(asked, ['original', 'replacement'], 'moving back asks again');

    type('@');
    await settle();
    equal(root.querySelector('.mentions [role="option"] span')?.textContent, 'src/');
    type('/');
    equal(root.querySelector('.mentions'), null, 'changing list kind clears older rows');
    dispose();
    await settle();
    deepEqual(asked, ['original', 'replacement'], 'unmount cancels a scheduled query');
  } finally {
    dispose();
    root.remove();
  }
});
