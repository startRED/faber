import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppEvent, type AppState } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { Segmented } from '../../ui/controls.tsx';
import { check, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 40));
const now = Date.now();

// Mounts the whole screen over a fake engine whose `remove` answers when the check says so, and whose state still lists
// the row until then: the stale read a refresh makes while the call is in flight.
async function withApp(run: (screen: { root: HTMLElement; hear: (e: AppEvent) => void; server: AppState; ask: HTMLButtonElement }) => Promise<void>, remove: () => Promise<void>) {
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const original = { state: api.state, remove: api.remove, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts };
  const storage = localStorage.getItem('faber.place');
  const server = {
    conversations: [{ id: 'doomed', title: 'Doomed chat', area: 'chat', updatedAt: now }, { id: 'kept', title: 'Kept chat', area: 'chat', updatedAt: now - 1 }],
    groups: [], projects: [], vendors: ['claude'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    models: [], running: { revision: 0, items: [] }, quotas: [], version: 'test',
    look: { vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } },
  } as unknown as AppState;
  let hear: ((event: AppEvent) => void) | undefined;
  api.state = async () => structuredClone(server);
  api.remove = remove;
  api.warm = async () => {}; api.routeDraft = async () => {}; api.setClock = async () => {}; api.setShellTexts = async () => {};
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat' }));
  const dispose = render(() => createComponent(App, { initial: server, inbox: { take: (listener) => { hear = listener; } } }), root);
  try {
    await settle();
    const row = root.querySelector('[data-conversation-id="doomed"]');
    ok(row, 'the conversation is listed before it is deleted');
    await run({ root, hear: (e) => hear?.(e), server, ask: row.querySelector<HTMLButtonElement>('.x')! });
  } finally {
    dispose(); root.remove(); Object.assign(api, original);
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
}

check('optimistic: deleting a conversation removes its row before the engine answers and a stale refresh cannot bring it back', async () => {
  let answer!: () => void;
  const calls: string[] = [];
  await withApp(async ({ root, hear, server, ask }) => {
    ask.click(); await settle();
    ask.click(); await settle();
    deepEqualCalls(calls, ['doomed']);
    equal(root.querySelector('[data-conversation-id="doomed"]'), null, 'the row is gone while the call is still pending');
    ok(root.querySelector('[data-conversation-id="kept"]'), 'the other row stays');
    hear({ type: 'list' }); await settle();
    equal(root.querySelector('[data-conversation-id="doomed"]'), null, 'a refresh that still lists the row does not resurrect it');
    server.conversations = server.conversations.filter((c) => c.id !== 'doomed');
    answer(); await settle();
    equal(root.querySelector('[data-conversation-id="doomed"]'), null);
  }, (id?: string) => { calls.push(String(id)); return new Promise<void>((done) => { answer = done; }); });
});

check('optimistic: a refused delete brings the row back where it was and says why', async () => {
  let refuse!: (reason: unknown) => void;
  await withApp(async ({ root, ask }) => {
    ask.click(); await settle();
    ask.click(); await settle();
    equal(root.querySelector('[data-conversation-id="doomed"]'), null, 'gone at once');
    refuse('error.chat.saveFailed'); await settle();
    const rows = [...root.querySelectorAll('[data-conversation-id]')].map((el) => el.getAttribute('data-conversation-id'));
    equal(rows.join(), 'doomed,kept', 'the row is back in its place');
    ok(document.querySelector('[role=alert]'), 'the failure is shown');
  }, () => new Promise<void>((_, no) => { refuse = no; }));
});

check('optimistic: a segmented choice shows the pick at once and goes back when the save fails', async () => {
  const root = document.createElement('div'); document.body.append(root);
  let fail!: () => void;
  const [value] = createSignal('a');
  const dispose = render(() => createComponent(Segmented, { id: 's', label: 'Pick', value: value(), options: [['a', 'A'], ['b', 'B']] as [string, string][], onPick: () => new Promise<void>((_, no) => { fail = no; }) }), root);
  try {
    const [a, b] = [...root.querySelectorAll('button')];
    b!.click();
    equal(b!.getAttribute('aria-pressed'), 'true', 'the pick shows before the save answers');
    fail(); await settle();
    equal(a!.getAttribute('aria-pressed'), 'true', 'the old choice is back after a refusal');
    equal(b!.getAttribute('aria-pressed'), 'false');
  } finally { dispose(); root.remove(); }
});

function deepEqualCalls(a: string[], b: string[]) { equal(a.join(), b.join()); }
