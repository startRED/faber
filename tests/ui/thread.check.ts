import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

check('thread: ten tool steps and narration fold into one activity row above the final answer', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const narration = 'Checking the files.\n\nRunning the checks.\n\n';
  const final = 'The requested change is complete.\n\nAll checks passed.';
  const [message, setMessage] = createSignal<Message>({
    id: 'activity-check', role: 'assistant', at: 0, status: 'streaming', text: narration + final,
    textBreaks: ['Checking the files.\n\n'.length, narration.length], activityEnd: narration.length, thinking: 'Consider the simplest change.',
    tools: Array.from({ length: 10 }, (_, i) => ({ id: String(i), name: 'Bash', input: { command: `check ${i}` }, ok: i < 9 ? true : undefined })),
  });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'activity-check', options: [], onBranch: () => {},
  }), root);
  try {
    const group = root.querySelector<HTMLDetailsElement>('.activity')!;
    equal(root.querySelectorAll('.activity').length, 1);
    equal(group.open, false);
    ok(root.querySelector('.activity-latest')!.textContent?.includes('check 9'), 'latest running command remains visible');
    setMessage({ ...message(), status: 'done' });
    equal(root.querySelectorAll('.activity').length, 1);
    equal(group.open, false, 'completion stays collapsed');
    equal(root.querySelector('.activity-latest'), null);
    ok(root.querySelector('.ai > .md')!.textContent?.includes(final.split('\n')[0]!));
    ok(!root.querySelector('.ai > .md')!.textContent?.includes('Checking the files'), 'narration is kept out of the final answer');
    group.open = true;
    ok(group.textContent?.includes('Checking the files'));
    equal(group.querySelectorAll('.tools > span').length, 10);
    // Older conversations without recorded boundaries keep their full text.
    setMessage({ ...message(), textBreaks: undefined, activityEnd: undefined });
    ok(root.querySelector('.ai > .md')!.textContent?.includes('Checking the files'));
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: with Auto the model name shows as soon as it is chosen, while the answer streams, and stays the same at the end', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'auto-name', role: 'assistant', at: 0, status: 'streaming', text: '', reason: 'reason.routing' });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'auto-name', options: [], onBranch: () => {},
  }), root);
  const header = () => root.querySelector<HTMLElement>('.model')!;
  const name = () => header().children[1]!.textContent;
  try {
    equal(name(), t('answer.answering'), 'while Auto is still choosing nothing is named yet');
    setMessage({ ...message(), vendor: 'claude', model: 'claude-haiku-5-5', effort: 'medium' });
    equal(name(), 'Haiku 5.5', 'the name shows before the first word');
    setMessage({ ...message(), text: 'Sunlight cont' });
    equal(name(), 'Haiku 5.5', 'and stays while the words arrive');
    setMessage({ ...message(), status: 'done' });
    equal(name(), 'Haiku 5.5', 'and is the same when the answer ends');
  } finally {
    dispose();
    root.remove();
  }
});

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
