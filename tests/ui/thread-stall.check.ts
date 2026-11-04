import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { App } from '../../ui/App.tsx';
import { api, type AppState, type Conversation, type Message } from '../../ui/api.ts';
import { check, frames, ok } from './check.ts';

check('thread stall: open and scroll 300 messages', async () => {
  const original = { ...api };
  const saved = new Map(['faber.place', 'faber.reading'].map((key) => [key, localStorage.getItem(key)]));
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0;z-index:100';
  document.body.append(root);
  const id = 'stall-300';
  const messages: Message[] = Array.from({ length: 300 }, (_, i) => ({
    id: `stall-${i}`, role: i % 2 ? 'assistant' : 'user', at: i + 1,
    text: `Message ${i + 1}. ` + 'Words that stay at the same reading position. '.repeat(10),
    ...(i % 2 ? { status: 'done' as const } : {}),
  }));
  const page = (): Conversation => ({ id, title: 'Long conversation', area: 'chat', sessions: {}, createdAt: 1, updatedAt: 1,
    earlier: 260, messages: messages.slice(-40) });
  let dispose = () => {};
  let timer: ReturnType<typeof setInterval> | undefined;
  try {
    const base = await original.state();
    const initial: AppState = { ...base, conversations: [{ id, title: 'Long conversation', area: 'chat', updatedAt: 1 }],
      groups: [], projects: [], vendors: ['claude'], running: { revision: 0, items: [] }, background: { revision: 0, items: [] }, quotas: [], usage: [],
      look: { ...base.look, motion: 'none' },
      settings: { ...base.settings, appearance: { ...base.look.appearance, motion: 'none' }, language: 'en', setup: 'done', tutorialDone: true } };
    Object.assign(api, { state: async () => initial, open: async () => page(), versions: async () => [], visualAnswers: async () => [],
      rewindRedos: async () => [], rewindPoints: async () => [], commands: async () => [], warm: async () => {}, seen: async () => {},
      notesSeen: async () => Number.MAX_SAFE_INTEGER, updateStatus: async () => ({ phase: 'none' }), onUpdate: async () => () => {},
      messagesBefore: async (_id: string, first: string) => {
        const at = messages.findIndex((message) => message.id === first);
        return messages.slice(Math.max(0, at - 40), at);
      } });
    localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id }));
    localStorage.removeItem('faber.reading');
    let last = performance.now(), longest = 0;
    timer = setInterval(() => { const now = performance.now(); longest = Math.max(longest, now - last - 16); last = now; }, 16);
    dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
    const wait = async (test: () => boolean, message: string) => {
      for (let i = 0; i < 200 && !test(); i++) await new Promise((done) => setTimeout(done, 20));
      ok(test(), message);
    };
    await wait(() => !!root.querySelector('[data-message-index="299"]'), 'the newest message must open');
    const thread = root.querySelector<HTMLDivElement>('.thread')!;
    for (const index of [220, 180, 140, 100, 60, 20, 0]) {
      thread.scrollTop = 0;
      thread.dispatchEvent(new Event('scroll'));
      await wait(() => !!thread.querySelector(`[data-message-index="${index}"]`), `message ${index} must load`);
      await frames();
    }
    await new Promise((done) => setTimeout(done, 30));
    (window as unknown as { threadStallMs: number }).threadStallMs = longest;
    console.info(`300-message open and scroll longest timer delay: ${longest.toFixed(1)} ms`);
  } finally {
    clearInterval(timer); dispose(); root.remove(); Object.assign(api, original);
    for (const [key, value] of saved) { if (value === null) localStorage.removeItem(key); else localStorage.setItem(key, value); }
  }
});
