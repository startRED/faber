import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { check, ok } from './check.ts';

check('thread: the last answer and its actions leave at least 48px above either composer height', async () => {
  for (const [width, height] of [[1440, 900], [1000, 700]]) {
    const root = document.createElement('div');
    root.style.cssText = `position:fixed;left:0;top:0;width:${width}px;height:${height}px`;
    root.dataset.motion = 'none';
    for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
    root.innerHTML = '<div class="work" style="height:100%"><div class="thread"><div class="col"></div></div><div class="dock"></div></div>';
    document.body.append(root);
    const messages: Message[] = Array.from({ length: 30 }, (_, i) => ({
      id: String(i), role: i % 2 ? 'assistant' : 'user', text: 'A long thread with enough text to scroll. '.repeat(10),
      at: 0, status: 'done', vendor: 'codex', model: 'gpt-6',
    }));
    let fill!: (text: string) => void;
    const disposeThread = render(() => createComponent(Thread, {
      messages, from: 0, conversationId: 'gap-check', options: [], onBranch: () => {},
    }), root.querySelector('.col')!);
    const disposeComposer = render(() => createComponent(Composer, {
      vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
      onChoice: () => {}, onSend: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
      onReady: (ready) => { fill = ready; },
    }), root.querySelector('.dock')!);
    try {
      for (const text of ['', 'A very long prompt. '.repeat(500)]) {
        fill(text);
        await new Promise((resolve) => setTimeout(resolve, 40));
        const thread = root.querySelector<HTMLElement>('.thread')!;
        thread.scrollTop = thread.scrollHeight;
        ok(thread.scrollTop > 0, 'the thread must actually overflow');
        const last = root.querySelector('.col')!.lastElementChild!;
        ok(last.querySelector('.actions'), 'the measured answer includes its action row');
        const gap = root.querySelector('.composer')!.getBoundingClientRect().top - last.getBoundingClientRect().bottom;
        ok(gap >= 48 && gap <= 72, `${width}x${height}: gap ${gap}px with ${text ? 'expanded' : 'empty'} composer`);
      }
    } finally {
      disposeThread();
      disposeComposer();
      root.remove();
    }
  }
});
