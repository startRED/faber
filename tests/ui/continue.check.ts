import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState, type Conversation, type SendRequest } from '../../ui/api.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const look = {
  vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
  appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
};

check('continue: a past project conversation takes a new message and its reply joins the same conversation', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { open: api.open, send: api.send, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const storage = localStorage.getItem('faber.place');
  const day = 86_400_000;
  const c: Conversation = {
    id: 'past-project', title: 'Old parser work', area: 'project', projectId: 'p1', createdAt: 1, updatedAt: Date.now() - 30 * day, sessions: { claude: 's1' },
    messages: [
      { id: 'q1', role: 'user', text: 'Split the parser', at: Date.now() - 30 * day },
      { id: 'r1', role: 'assistant', text: 'Split it in two files.', at: Date.now() - 30 * day, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5' },
    ],
    queued: [],
  };
  const initial = {
    conversations: [{ id: c.id, title: c.title, area: c.area, projectId: 'p1', updatedAt: c.updatedAt }],
    projects: [{ id: 'p1', name: 'Parser', path: 'E:/parser' }], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } }, running: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  const sent: SendRequest[] = [];
  api.open = async () => c;
  api.send = async (request) => {
    sent.push(request);
    c.messages = [...c.messages, { id: 'q2', role: 'user', text: request.text, at: Date.now() }, { id: 'r2', role: 'assistant', text: '', at: Date.now(), status: 'streaming', vendor: 'claude' }];
    return c.id;
  };
  api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  localStorage.setItem('faber.place', JSON.stringify({ area: 'project', projectId: 'p1', id: c.id }));
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  try {
    await settle();
    ok(host.textContent!.includes('Split it in two files.'), 'the past conversation is open');
    const field = host.querySelector<HTMLTextAreaElement>('.dock .composer textarea');
    ok(field, 'it has the project composer');
    field!.value = 'Now add tests for both';
    field!.dispatchEvent(new InputEvent('input', { bubbles: true }));
    host.querySelector('.dock form.composer')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    deepEqual(sent.map((r) => [r.conversationId, r.area, r.projectId, r.text]), [[c.id, 'project', 'p1', 'Now add tests for both']], 'the message goes to the same conversation, in its project');
    hear({ type: 'message', conversationId: c.id, message: { id: 'r2', role: 'assistant', text: 'Added tests for both files.', at: Date.now(), status: 'done', vendor: 'claude' } });
    await settle();
    const thread = host.textContent!;
    ok(thread.includes('Split it in two files.') && thread.includes('Added tests for both files.'), 'the reply joins the same conversation, after the old one');
    equal(host.querySelectorAll('.dock .composer textarea').length, 1, 'the composer stays for the next message');
  } finally {
    dispose();
    host.remove();
    Object.assign(api, before);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});
