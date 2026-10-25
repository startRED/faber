import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { applyLook } from '../../ui/appearance.ts';
import { api, type Aside, type AppEvent, type AppState, type Conversation, type Message } from '../../ui/api.ts';
import { btw, QuickQuestion } from '../../ui/QuickQuestion.tsx';
import { forgetDraft, kept } from '../../ui/unsent.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

check('quick question: /btw reads as a side question only at the start of the box', () => {
  equal(btw('/btw why that file?'), 'why that file?');
  equal(btw('  /BTW  how is it going?\n'), 'how is it going?');
  equal(btw('/btw'), '', 'alone it only opens the tab');
  equal(btw('/btwx'), undefined);
  equal(btw('tell me /btw why'), undefined, 'anywhere else it is part of the message');
});

const look = {
  vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
  appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
};

check('side chat: a late failed request cannot restore a draft after its conversation was deleted', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const conversation: Conversation = { id: 'check-deleted-aside', title: 'Deleted conversation', area: 'chat', sessions: {}, createdAt: 1, updatedAt: 1,
    messages: [{ id: 'answer', role: 'assistant', text: 'An answer', vendor: 'claude', status: 'done', at: 1 }],
  };
  const key = `aside.${conversation.id}`;
  const before = { askAside: api.askAside, asides: api.asides };
  let reject = (_error: Error) => {};
  let asked = false;
  api.asides = async () => [];
  api.askAside = () => new Promise((_resolve, fail) => { asked = true; reject = fail; });
  const dispose = render(() => createComponent(QuickQuestion, { conversation, options: [], onClose: () => {}, onTell: () => {} }), host);
  try {
    await settle();
    const field = host.querySelector<HTMLTextAreaElement>('#aside-box')!;
    field.value = 'Private side question';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await settle();
    ok(asked);
    ok(await forgetDraft(key));
    dispose();
    reject(new Error('request ended after deletion'));
    await settle();
    equal(kept(key), '', 'the old request cannot recreate explicitly deleted private words');
  } finally {
    dispose(); host.remove();
    Object.assign(api, before);
    await forgetDraft(key);
  }
});

for (const entry of ['button', 'menu', 'send'] as const) {
  check(`recap: ${entry} answers on the side of an idle conversation and leaves its thread alone`, async () => {
    const settle = () => new Promise((resolve) => setTimeout(resolve, 50));
    const host = document.createElement('div');
    document.body.append(host);
    const before = { open: api.open, send: api.send, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, interrupt: api.interrupt, askAside: api.askAside, asides: api.asides, commands: api.commands };
    const storage = localStorage.getItem('faber.place');
    const c: Conversation = {
      id: `recap-${entry}`, title: 'Parser', area: entry === 'menu' ? 'project' : 'chat', projectId: entry === 'menu' ? 'p1' : undefined,
      createdAt: 1, updatedAt: 1, sessions: { claude: 's1' }, queued: [],
      messages: [{ id: 'q', role: 'user', text: 'Refactor the parser', at: 0 },
        { id: 'a', role: 'assistant', text: 'Parser refactored. Tests still open.', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5' }],
    };
    const initial = { conversations: [{ id: c.id, title: c.title, area: c.area, projectId: c.projectId, updatedAt: 1 }],
      projects: [{ id: 'p1', name: 'Parser', path: '/parser' }], vendors: ['claude'],
      settings: { setup: 'done', tutorialDone: true, language: entry === 'menu' ? 'pt-BR' : 'en', appearance: { base: 'sala' } },
      running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
    } as AppState;
    const snapshot = JSON.stringify(c);
    const asked: string[] = [];
    let sent = 0;
    let stopped = 0;
    api.open = async () => c; api.state = async () => initial;
    api.send = async () => { sent++; return c.id; };
    api.interrupt = async () => { stopped++; };
    api.warm = async () => {}; api.routeDraft = async () => {};
    api.setClock = async () => {}; api.setShellTexts = async () => {};
    api.asides = async () => []; api.commands = async () => [];
    api.askAside = async (id, question) => { equal(id, c.id); asked.push(question); return `a-${entry}`; };
    localStorage.setItem('faber.place', JSON.stringify({ area: c.area, projectId: c.projectId, id: c.id }));
    let hear!: (event: AppEvent) => void;
    applyLook(initial.look, host);
    const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
    try {
      await settle();
      const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
      if (entry === 'button') {
        ok(host.querySelector('.topbar .pane-aside'), 'the topbar offers a quick question with nothing running');
        host.querySelector<HTMLButtonElement>('.pane-aside')!.click();
        await settle();
        const box = host.querySelector<HTMLTextAreaElement>('#aside-box')!;
        box.value = 'Why the parser?';
        box.dispatchEvent(new InputEvent('input', { bubbles: true }));
        host.querySelector<HTMLButtonElement>('.aside-recap')!.click();
      } else {
        field.value = '/recap'; field.setSelectionRange(6, 6);
        field.dispatchEvent(new InputEvent('input', { bubbles: true }));
        if (entry === 'menu') {
          await new Promise((resolve) => setTimeout(resolve, 180));
          const option = host.querySelector<HTMLElement>('.mentions [role="option"]')!;
          equal(option.querySelector('span')?.textContent, '/recap');
          equal(option.querySelector('.mention-note')?.textContent, t('aside.recap'));
          option.dispatchEvent(new MouseEvent('mousedown', { button: 0, bubbles: true, cancelable: true }));
        } else host.querySelector('form.composer')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
      }
      await settle();
      deepEqual(asked, [t('aside.recapQuestion')], 'the fixed question follows the person’s language');
      if (entry === 'button') equal(host.querySelector<HTMLTextAreaElement>('#aside-box')!.value, 'Why the parser?', 'a recap keeps the person’s own draft');
      const aside: Aside = { id: `a-${entry}`, question: asked[0], answer: 'You asked for a refactor. The parser is done. Tests remain.', vendor: 'claude', state: 'done' };
      hear({ type: 'asides', conversationId: c.id, asides: [aside] });
      await settle();
      equal(host.querySelector('.aside-answer')?.textContent?.trim(), aside.answer);
      ok(![...host.querySelectorAll('article')].some((a) => a.textContent!.includes(aside.answer)), 'only the side panel holds the recap');
      equal(field.value, '', 'nothing is inserted in the main box');
      equal(sent, 0); equal(stopped, 0);
      equal(JSON.stringify(c), snapshot, 'messages, queue and session are unchanged');
    } finally {
      dispose(); host.remove(); Object.assign(api, before);
      if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
    }
  });
}

check('quick question: asked while the agent works, it leaves the turn, its queue and its session alone', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { open: api.open, send: api.send, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, interrupt: api.interrupt, askAside: api.askAside, stopAside: api.stopAside, asides: api.asides };
  const storage = localStorage.getItem('faber.place');
  const answer: Message = { id: 'answer', role: 'assistant', text: 'Refactoring the parser', at: 1, status: 'streaming', vendor: 'claude', model: 'claude-sonnet-5-5' };
  const c: Conversation = {
    id: 'aside-app', title: 'Parser', area: 'chat', createdAt: 1, updatedAt: 1, sessions: { claude: 's1' },
    messages: [{ id: 'question', role: 'user', text: 'Refactor the parser', at: 0 }, answer],
    queued: [{ id: 'q1', at: 2, text: 'Then add tests', choice: { vendor: 'claude' } }],
  };
  const initial = { conversations: [{ id: c.id, title: c.title, area: c.area, updatedAt: 1 }], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  const sent: string[] = [];
  const asked: [string, string, string | undefined][] = [];
  let interrupted = 0;
  api.open = async () => c;
  api.send = async (request) => { sent.push(request.text); return c.id; };
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.interrupt = async () => { interrupted++; };
  api.asides = async () => [];
  api.askAside = async (id, question, model) => { asked.push([id, question, model]); return 'a1'; };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: c.id }));
  let hear!: (event: AppEvent) => void;
  applyLook(initial.look, host);
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  try {
    await settle();
    const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = '/btw why that file?';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    host.querySelector('form.composer')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    deepEqual(sent, [], 'the question never reaches the conversation');
    deepEqual(asked, [[c.id, 'why that file?', undefined]], 'it is asked on the side, on the conversation\'s own model');
    equal(interrupted, 0, 'the turn keeps running');
    equal(field.value, '', 'the box is free for the agent again');
    const pane = host.querySelector('[data-surface="aside"]');
    ok(pane, 'the running column shows the quick question');
    equal(host.querySelector('#running-title')!.textContent, t('aside.title'));

    const running: Aside = { id: 'a1', question: 'why that file?', answer: '', vendor: 'claude', model: 'claude-sonnet-5-5', state: 'running' };
    hear({ type: 'asides', conversationId: c.id, asides: [running] });
    await settle();
    ok(host.textContent!.includes(t('aside.looking')), 'it says it is looking');
    ok(host.querySelector('.side-compose .send.stop') && !host.querySelector('.side-compose button[type="submit"]'), 'one question at a time: the box offers Stop');
    equal(host.querySelector('.aside .you')!.textContent, 'why that file?', 'the question sits in its own bubble');
    equal(host.querySelector('.aside-reply .vm b')!.textContent, 'Sonnet 5.5', 'the answer carries the model mark');
    hear({ type: 'asides', conversationId: c.id, asides: [{ ...running, answer: 'It holds the **tokenizer**.', state: 'done' }] });
    await settle();
    ok(host.querySelector('.aside-answer strong')?.textContent === 'tokenizer', 'the answer reads like any reply');
    // The thread and its queue are as they were: nothing of the side question is in them.
    ok(!host.querySelector('.thread')!.textContent!.includes('why that file?'), 'the thread never shows the question');
    ok(host.textContent!.includes('Then add tests'), 'the queued message is still there');

    const sendBack = [...host.querySelectorAll<HTMLButtonElement>('.aside button')].find((b) => b.textContent === t('aside.send'))!;
    sendBack.click();
    await settle();
    equal(field.value, 'It holds the **tokenizer**.', '"Send to the agent" puts the answer in the main box');
    deepEqual(sent, [], 'and sends nothing by itself');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('quick question: before the agent has answered, the tab says when it can be asked', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { open: api.open, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, askAside: api.askAside, asides: api.asides };
  const storage = localStorage.getItem('faber.place');
  const c: Conversation = { id: 'aside-empty', title: 'New', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {}, messages: [{ id: 'q', role: 'user', text: 'Hello', at: 0 }], queued: [] };
  const initial = { conversations: [{ id: c.id, title: c.title, area: c.area, updatedAt: 1 }], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  let asked = 0;
  api.open = async () => c;
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.asides = async () => [];
  api.askAside = async () => { asked++; return 'a1'; };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: c.id }));
  applyLook(initial.look, host);
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
  try {
    await settle();
    const field = host.querySelector<HTMLTextAreaElement>('.composer textarea')!;
    field.value = '/btw how is it going?';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    host.querySelector('form.composer')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(asked, 0, 'nothing runs before the agent has answered');
    equal(field.value, '/btw how is it going?', 'the words stay in the box, never lost');
    ok(host.querySelector('[data-surface="aside"]')!.textContent!.includes(t('aside.none')), 'it says to ask once the agent has answered');
    ok(!host.querySelector('#aside-box'), 'there is no box to ask in yet');
    field.value = '/recap';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    host.querySelector('form.composer')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(asked, 0, 'a recap cannot run before the first answer either');
    equal(field.value, '/recap', 'the unasked recap stays in the box');
    ok(!host.querySelector('.aside-recap'), 'there is no recap button before the first answer');
    equal(host.querySelector('.side-head .back'), null, 'it is a view of its own, with no way back to the running list');
    equal(host.querySelector('.pane-aside')!.getAttribute('aria-pressed'), 'true', 'the topbar says the quick question is on screen');
    const ctrlSemicolon = () => window.dispatchEvent(new KeyboardEvent('keydown', { key: ';', ctrlKey: true, bubbles: true, cancelable: true }));
    ctrlSemicolon();
    await settle();
    equal(host.querySelector('[data-surface="aside"]'), null, 'Ctrl+; closes it');
    ctrlSemicolon();
    await settle();
    ok(host.querySelector('[data-surface="aside"]'), 'and Ctrl+; opens it again');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('side chat: the kept thread comes back when it opens again, and a follow-up, Esc, Clear and the draft work', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { open: api.open, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, askAside: api.askAside, stopAside: api.stopAside, asides: api.asides, clearAsides: api.clearAsides };
  const storage = localStorage.getItem('faber.place');
  const c: Conversation = {
    id: 'side-kept', title: 'Parser', area: 'chat', createdAt: 1, updatedAt: 1, sessions: { claude: 's1' }, queued: [],
    messages: [{ id: 'q', role: 'user', text: 'Refactor the parser', at: 0 }, { id: 'a', role: 'assistant', text: 'Done.', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5' }],
  };
  const initial = { conversations: [{ id: c.id, title: c.title, area: c.area, updatedAt: 1 }], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  // What the engine keeps with the conversation: it outlives the view, so it is read again each time the view opens.
  let kept: Aside[] = [{ id: 'k1', question: 'Why the parser?', answer: 'It was **slow**.', vendor: 'claude', model: 'claude-sonnet-5-5', state: 'done' }];
  const asked: string[] = [];
  const stopped: string[] = [];
  let cleared = 0;
  api.open = async () => c; api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  api.asides = async () => kept;
  api.askAside = async (_id, question) => { asked.push(question); return 'k2'; };
  api.stopAside = async (id) => { stopped.push(id); };
  api.clearAsides = async () => { cleared++; kept = []; };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: c.id }));
  localStorage.removeItem('faber.unsent.aside.side-kept');
  let hear!: (event: AppEvent) => void;
  applyLook(initial.look, host);
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  const toggle = () => window.dispatchEvent(new KeyboardEvent('keydown', { key: ';', ctrlKey: true, bubbles: true, cancelable: true }));
  try {
    await settle();
    toggle();
    await settle();
    equal(host.querySelector('.aside-answer strong')?.textContent, 'slow', 'the kept answer is there as soon as it opens');
    ok(!host.querySelector('.side-empty'), 'a thread has no suggestions over it');
    const box = () => host.querySelector<HTMLTextAreaElement>('#aside-box')!;
    equal(box().placeholder, t('aside.followUp'), 'with a thread the box asks for a follow-up');
    box().value = 'And the tests?';
    box().dispatchEvent(new InputEvent('input', { bubbles: true }));
    toggle();
    await settle();
    equal(host.querySelector('[data-surface="aside"]'), null, 'closed');
    toggle();
    await settle();
    equal(box().value, 'And the tests?', 'the draft is kept across closing');
    box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await settle();
    deepEqual(asked, ['And the tests?'], 'Enter asks');
    equal(box().value, '', 'and empties the box');
    kept = [...kept, { id: 'k2', question: 'And the tests?', answer: '', vendor: 'claude', model: 'claude-sonnet-5-5', state: 'running' }];
    hear({ type: 'asides', conversationId: c.id, asides: kept });
    await settle();
    box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await settle();
    deepEqual(stopped, ['k2'], 'Esc stops the answer under way');
    ok(host.querySelector('[data-surface="aside"]'), 'and leaves the view open');
    kept = [kept[0]!, { ...kept[1]!, state: 'stopped' }];
    hear({ type: 'asides', conversationId: c.id, asides: kept });
    await settle();
    equal(host.querySelectorAll('.aside').length, 2);
    const clear = [...host.querySelectorAll<HTMLButtonElement>('.side-head button')].find((b) => b.textContent === t('aside.clear'))!;
    clear.click();
    await settle();
    equal(cleared, 1, 'Clear in the head empties the kept thread');
    hear({ type: 'asides', conversationId: c.id, asides: [] });
    await settle();
    const chips = [...host.querySelectorAll<HTMLButtonElement>('.side-empty .chip')].map((b) => b.textContent);
    deepEqual(chips, [t('aside.recap'), t('aside.left'), t('aside.lastStep')], 'an empty side chat suggests three questions');
    equal(box().placeholder, t('aside.placeholder'));
    host.querySelectorAll<HTMLButtonElement>('.side-empty .chip')[1]!.click();
    await settle();
    equal(asked.at(-1), t('aside.leftQuestion'), 'a suggestion asks at once');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
    localStorage.removeItem('faber.unsent.aside.side-kept');
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});
