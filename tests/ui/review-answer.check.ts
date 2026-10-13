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
      if (!tools) equal(root.querySelector('.activity'), null, 'text blocks alone are not work steps');
    }
    setMessage({ ...message(), activityEnd: '🔎 Explanation.'.length });
    const answer = root.querySelector('.ai > .md')!.textContent!;
    ok(answer.includes('First result.') && answer.includes('Second result.'), 'all final blocks after the last tool stay visible');
    ok(!answer.includes('Explanation.'), 'only proven pre-tool narration folds');
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
    tools: [{ id: 'first', name: 'Bash', input: { command: 'check one' } }] });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0,
    conversationId: 'review', options: [], onBranch: () => {} }), root);
  try {
    const status = root.querySelector<HTMLElement>('.activity-latest')!;
    equal(status.getAttribute('role'), 'status');
    equal(status.getAttribute('aria-live'), 'polite');
    equal(status.getAttribute('aria-atomic'), 'true');
    equal(status.closest('[aria-busy="true"]'), null);
    setMessage({ ...message(), tools: [{ id: 'second', name: 'Bash', input: { command: 'check two' } }] });
    ok(status.textContent!.includes('check two'), 'the existing live region updates for a new step');
  } finally { dispose(); root.remove(); }
});

check('review 7: Copy writes every generated block, including code and folded narration', async () => {
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

check('review 15: activity counts use singular and plural forms in every hand-written locale', () => {
  const previous = lang();
  try {
    for (const [language, expected] of [ ['en', 'Worked · 1 step · 1 command'], ['de', 'Bearbeitet · 1 Schritt · 1 Befehl'],
      ['es', 'Trabajo realizado · 1 paso · 1 comando'], ['fr', 'Travail effectué · 1 étape · 1 commande'], ['pt-BR', 'Trabalho realizado · 1 etapa · 1 comando'] ]) {
      useLanguage(language);
      const root = document.createElement('div');
      const dispose = render(() => createComponent(Thread, { messages: [{ id: 'review', role: 'assistant', at: 0, status: 'done', text: 'Done.',
        tools: [{ id: 'tool', name: 'Bash', input: 'check', ok: true }] }], from: 0,
        conversationId: 'review', options: [], onBranch: () => {} }), root);
      try { equal(root.querySelector('summary')!.textContent, expected, language); }
      finally { dispose(); }
    }
  } finally { useLanguage(previous); }
});
