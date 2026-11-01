import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { lang, useLanguage } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

check('review 1: every block of a tool-free or legacy multi-block answer stays prominent', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const text = '🔎 Explanation.\n\nFirst result.\n\nSecond result.';
  const [message, setMessage] = createSignal<Message>({ id: 'review', role: 'assistant', at: 0, status: 'done', text,
    textBreaks: ['🔎 Explanation.\n\n'.length, '🔎 Explanation.\n\nFirst result.\n\n'.length] });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0,
    conversationId: 'review', options: [], onBranch: () => {} }), root);
  try {
    for (const tools of [undefined, [{ id: 'tool', name: 'Bash', input: 'check', ok: true }]]) {
      setMessage({ ...message(), tools });
      const answer = root.querySelector('.ai > .md')!.textContent!;
      for (const block of ['🔎 Explanation.', 'First result.', 'Second result.']) ok(answer.includes(block), `${block} must remain visible`);
      if (!tools) equal(root.querySelector('.steps'), null, 'text blocks alone are not work steps');
    }
    setMessage({ ...message(), activityEnd: '🔎 Explanation.'.length });
    const answer = [...root.querySelectorAll('.ai > .md')].map((part) => part.textContent).join('');
    ok(answer.includes('First result.') && answer.includes('Second result.'), 'all final blocks after the last tool stay visible');
    ok(answer.includes('Explanation.'), 'the words before the step remain visible');
    for (const end of ['🔎 Explanation.'.length, -1, 9999]) {
      setMessage({ ...message(), text: '🔎 Explanation.', activityEnd: end });
      ok(root.querySelector('.ai > .md')!.textContent!.includes('🔎 Explanation.'), 'tool-terminated turns and invalid boundaries keep the text');
    }
  } finally { dispose(); root.remove(); }
});

check('review 6: current activity is a polite atomic status outside any busy ancestor', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'review', role: 'assistant', at: 0, status: 'streaming', text: '',
    tools: [] });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0,
    conversationId: 'review', options: [], onBranch: () => {} }), root);
  try {
    const status = root.querySelector<HTMLElement>('.activity-latest')!;
    equal(status.getAttribute('role'), 'status');
    equal(status.getAttribute('aria-live'), 'polite');
    equal(status.getAttribute('aria-atomic'), 'true');
    equal(status.closest('[aria-busy="true"]'), null);
    setMessage({ ...message(), tools: [{ id: 'second', name: 'Bash', input: { command: 'check two' } }] });
    equal(root.querySelector('.activity-latest'), null, 'the running step itself replaces the status');
    ok(root.querySelector('.step')?.textContent?.includes('check two'), 'the running step remains visible');
  } finally { dispose(); root.remove(); }
});

check('review 7: Copy writes every generated block, including code and words before steps', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const text = 'Explanation.\n\n```js\nconst answer = 42;\n```\n\nAll done.';
  const original = navigator.clipboard.writeText;
  let copied = '';
  navigator.clipboard.writeText = async (value) => { copied = value; };
  const dispose = render(() => createComponent(Thread, { messages: [{ id: 'review', role: 'assistant', at: 0, status: 'done', text,
    textBreaks: [text.indexOf('All done.')], tools: [{ id: 'tool', name: 'Bash', input: 'check', ok: true }] }], from: 0,
    conversationId: 'review', options: [], onBranch: () => {} }), root);
  try {
    root.querySelector<HTMLButtonElement>('.actions button')!.click();
    await Promise.resolve();
    equal(copied, text);
  } finally { navigator.clipboard.writeText = original; dispose(); root.remove(); }
});

check('review 15: completed steps use localized past tense in every hand-written locale', () => {
  const previous = lang();
  try {
    for (const [language, expected] of [['en', 'Ran'], ['de', 'Ausgeführt'], ['es', 'Ejecutó'], ['fr', 'A exécuté'], ['pt-BR', 'Executou']]) {
      useLanguage(language);
      const root = document.createElement('div');
      const dispose = render(() => createComponent(Thread, { messages: [{ id: 'review', role: 'assistant', at: 0, status: 'done', text: 'Done.',
        tools: [{ id: 'tool', name: 'Bash', input: 'check', ok: true }] }], from: 0,
        conversationId: 'review', options: [], onBranch: () => {} }), root);
      try { equal(root.querySelector('.step-line b')!.textContent, expected, language); }
      finally { dispose(); }
    }
  } finally { useLanguage(previous); }
});
