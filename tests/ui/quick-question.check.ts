import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type Aside, type AppEvent, type AppState, type Conversation, type Message } from '../../ui/api.ts';
import { btw } from '../../ui/QuickQuestion.tsx';
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
    equal(host.querySelector<HTMLButtonElement>('.side-write button[type="submit"]')!.disabled, true, 'one question at a time');
    hear({ type: 'asides', conversationId: c.id, asides: [{ ...running, answer: 'It holds the **tokenizer**.', state: 'done' }] });
    await settle();
    ok(host.querySelector('.aside-answer strong')?.textContent === 'tokenizer', 'the answer reads like any reply');
    // The thread and its queue are as they were: nothing of the side question is in them.
    ok(!host.querySelector('.thread, main')!.textContent!.includes('why that file?'), 'the thread never shows the question');
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
    host.querySelector<HTMLButtonElement>('.side-head .back')!.click();
    await settle();
    equal(host.querySelector('#running-title')!.textContent, t('run.title'), 'the back arrow returns to the running list');
    ok(host.querySelector('.rc-aside'), 'the list\'s head opens it again');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});
