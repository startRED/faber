import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppEvent, type AppState, type BackgroundCommand, type Conversation } from '../../ui/api.ts';
import { Commands } from '../../ui/Commands.tsx';
import { t, useLanguage } from '../../ui/i18n.ts';
import { store } from '../../ui/running.ts';
import { createHold } from '../../ui/quiet.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const command = (id: string, over: Partial<BackgroundCommand> = {}): BackgroundCommand => ({ id, conversationId: 'commands-check', vendor: 'codex', command: 'npm run dev', started: 1, output: 'Server ready', state: 'running', stoppable: true, ...over });
const look = {
  vars: {}, scheme: 'dark', motion: 'none', checks: [], notices: [], effects: { ambient: false, texture: false, glow: false },
  appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } },
};

check('background commands: a game holds only the newest capped output snapshot', () => {
  const delivered: AppEvent[] = [];
  const hold = createHold((event) => delivered.push(event));
  hold.hold(true);
  for (let revision = 1; revision <= 100; revision++) {
    hold.push({ type: 'background', revision, items: [command('cap', { output: 'x'.repeat(8192) })] });
  }
  hold.push({ type: 'background', revision: 1, items: [] });
  equal(hold.waiting, 1, 'a game cannot retain one full snapshot per event');
  equal(delivered.length, 0);
  hold.hold(false);
  equal(delivered.length, 1);
  equal(delivered[0].type === 'background' && delivered[0].revision, 100);
});

check('background commands: the column opens its command view, follows events and stops only the chosen command at 720 px', async () => {
  useLanguage('en');
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const host = document.createElement('div');
  document.body.append(host);
  const before = { open: api.open, state: api.state, warm: api.warm, routeDraft: api.routeDraft, setClock: api.setClock, setShellTexts: api.setShellTexts, stopBackground: api.stopBackground, send: api.send };
  const storage = localStorage.getItem('faber.place');
  const width = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 720 });
  window.dispatchEvent(new Event('resize'));
  const c: Conversation = { id: 'commands-check', title: 'Preview', area: 'chat', createdAt: 1, updatedAt: 1, sessions: {}, messages: [{ id: 'q', role: 'user', at: 1, text: 'Start the preview' }] };
  const initial = {
    conversations: [{ id: c.id, title: c.title, area: c.area, updatedAt: 1 }], projects: [], vendors: ['codex'],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    running: { revision: 0, items: [{ conversationId: c.id, turnId: 'waiting', title: c.title, area: 'chat', state: 'needsYou', since: 0, permissions: [{ requestId: 'pending', tool: 'Bash', input: {} }] }] },
    background: { revision: 0, items: [] }, quotas: [], usage: [], models: [], version: 'test', look,
  } as AppState;
  api.open = async () => c; api.state = async () => initial;
  api.warm = async () => {}; api.routeDraft = async () => {};
  api.setClock = async () => {}; api.setShellTexts = async () => {};
  let sent = 0;
  api.send = async () => { sent++; return c.id; };
  const stopped: string[] = [];
  api.stopBackground = async (id) => {
    stopped.push(id);
    if (stopped.length === 1) throw 'error.backgroundStop';
  };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: c.id }));
  const revision = store.list().revision;
  store.offer({ revision: revision + 1, items: initial.running.items });
  let hear!: (event: AppEvent) => void;
  const dispose = render(() => createComponent(App, { initial, inbox: { take: (listener?: (event: AppEvent) => void) => { if (listener) hear = listener; } } }), host);
  try {
    await settle();
    equal(host.querySelector('#running')?.getAttribute('aria-modal'), null);
    const control = host.querySelector<HTMLButtonElement>('.rc-commands')!;
    ok(control, 'Commands lives in the running column head');
    equal(control.querySelector('.num'), null, 'zero leaves no count');
    control.click();
    await settle();
    const column = host.querySelector<HTMLElement>('#running')!;
    equal(column.getAttribute('aria-modal'), 'true', 'the same side-view modal transition is used');
    ok(column.contains(document.activeElement));
    ok(host.querySelector('.commands-view .rc-empty'), 'the empty view uses the column empty state');
    const a = command('opaque-a');
    const b = command('opaque-b', { vendor: 'claude', command: 'sleep 60', stoppable: false });
    hear({ type: 'background', revision: 2, items: [a, b] });
    await settle();
    equal(host.querySelectorAll('.command-row').length, 2);
    equal(host.querySelectorAll('.command-stop').length, 1, 'a CLI without a stop route offers no Stop');
    const stop = host.querySelector<HTMLButtonElement>('.command-stop')!;
    stop.focus();
    hear({ type: 'background', revision: 3, items: [{ ...a, output: '<script>plain output</script>\nReady' }, b] });
    await settle();
    equal(host.querySelector('.command-stop'), stop, 'output updates keep the focused row alive');
    equal(document.activeElement, stop);
    equal(host.querySelector('.command-output script'), null, 'output is plain text');
    ok(host.querySelector('.command-output')!.textContent!.includes('<script>'));
    hear({ type: 'background', revision: 1, items: [] });
    hear({ type: 'list' });
    await settle();
    equal(host.querySelectorAll('.command-row').length, 2, 'neither stale events nor a stale state read replace newer commands');
    stop.click();
    await settle();
    equal(host.querySelector('[role="alert"]')!.textContent, t('error.backgroundStop'));
    equal(stop.disabled, false, 'a failed stop can be retried');
    stop.click();
    await settle();
    deepEqual(stopped, ['opaque-a', 'opaque-a'], 'only the engine-provided opaque id is sent');
    equal(sent, 0, 'Stop never sends a message to the agent');
    hear({ type: 'background', revision: 4, items: [{ ...a, state: 'stopped' }, { ...b, state: 'done' }] });
    await settle();
    equal(host.querySelectorAll('.command-stop').length, 0);
    ok(host.textContent!.includes(t('commands.stopped')));
    column.querySelector<HTMLButtonElement>('.back')!.click();
    await settle();
    equal(host.querySelector('.commands-view'), null, 'Back releases command output DOM');
    equal(host.querySelector('.rc-commands .num'), null, 'finished commands are not counted');
    hear({ type: 'background', revision: 5, items: [a] });
    await settle();
    equal(host.querySelector('.rc-commands .num')!.textContent, '1');
  } finally {
    dispose(); host.remove(); Object.assign(api, before);
    store.offer({ revision: revision + 2, items: [] });
    Object.defineProperty(window, 'innerWidth', width); window.dispatchEvent(new Event('resize'));
    if (storage === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', storage);
  }
});

check('background commands: thirty-two capped outputs use one text node each and release their DOM on close', () => {
  const host = document.createElement('div');
  document.body.append(host);
  const output = `${'x'.repeat(407)}\n`.repeat(19) + 'x'.repeat(440);
  equal(new TextEncoder().encode(output).length, 8192);
  equal(output.split('\n').length, 20);
  const items = Array.from({ length: 32 }, (_, i) => command(`cap-${i}`, { output }));
  const started = performance.now();
  const dispose = render(() => createComponent(Commands, { items, conversations: [], onBack: () => {}, onClose: () => {} }), host);
  try {
    equal(host.querySelectorAll('.command-row').length, 32);
    const outputs = [...host.querySelectorAll('.command-output')];
    equal(outputs.length, 32);
    equal(outputs.reduce((n, pre) => n + pre.textContent!.length, 0), 32 * 8192);
    for (const pre of outputs) {
      equal(pre.childElementCount, 0, 'no per-line elements or markup parser');
      equal(pre.childNodes.length, 1, 'each bounded output is one text node');
    }
    ok(host.querySelectorAll('*').length < 700, 'output lines add no element tree');
    console.info(`Background commands DOM: 32 x 8192 bytes / 20 lines; ${host.querySelectorAll('*').length} elements; ${Math.round(performance.now() - started)} ms to render; output text at most 524288 bytes as UTF-16.`);
  } finally {
    dispose();
    equal(host.childNodes.length, 0, 'closing releases all output nodes');
    host.remove();
  }
});
