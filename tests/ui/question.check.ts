import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api } from '../../ui/api.ts';
import { askedQuestions, Asks } from '../../ui/Ask.tsx';
import { useLanguage } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// The assistant's own questions: a card in its message, answered by tapping (docs/product-map.md, "Question card").

const settle = (ms = 20) => new Promise((resolve) => setTimeout(resolve, ms));

const INPUT = {
  questions: [
    { question: 'Which stack?', header: 'Stack', multiSelect: false, options: [{ label: 'Rust', description: 'Fast' }, { label: 'Go', description: 'Simple' }] },
    { question: 'Which extras?', header: 'Extras', multiSelect: true, options: [{ label: 'Docs' }, { label: 'Tests' }, { label: 'CI' }] },
  ],
};

async function withCard(body: (root: HTMLElement, calls: unknown[][]) => Promise<void>, input: unknown = INPUT) {
  const originals = { ...api };
  useLanguage('en');
  const calls: unknown[][] = [];
  api.answerQuestion = async (...args) => void calls.push(['question', ...args]);
  api.answer = async (...args) => void calls.push(['answer', ...args]);
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => createComponent(Asks, { conversationId: 'c1', turnId: 't1', permissions: [{ requestId: 'r1', tool: 'AskUserQuestion', input }] }),
    root,
  );
  try {
    await settle();
    await body(root, calls);
  } finally {
    dispose();
    root.remove();
    Object.assign(api, originals);
  }
}

const click = (el: Element | null | undefined) => (el as HTMLElement).click();
const options = (root: HTMLElement, question: number) => [...root.querySelectorAll('fieldset.q')[question].querySelectorAll<HTMLElement>('.q-option')];

check('question card: a malformed question list shows nothing to answer', () => {
  deepEqual(askedQuestions(undefined), []);
  deepEqual(askedQuestions({ questions: 'no' }), []);
  deepEqual(askedQuestions({ questions: [{ options: [] }, { question: 'Ok?', options: [{ label: 'Yes' }, { nope: 1 }] }] }).map((q) => q.options.length), [1]);
});

check('question card: the agent’s question is a card with options to tap, and Send waits for every answer', () => withCard(async (root, calls) => {
  const card = root.querySelector('[data-testid="question-card"]')!;
  ok(card, 'a question card, not a permission card');
  equal(root.querySelectorAll('fieldset.q').length, 2);
  equal(options(root, 0).map((o) => o.querySelector('span')?.textContent).join(), 'Rust,Go');
  const send = [...root.querySelectorAll<HTMLButtonElement>('.btn.primary')].find((b) => b.textContent?.trim() === 'Send answers')!;
  ok(send.disabled, 'nothing chosen yet');
  click(options(root, 0)[1]);
  await settle();
  ok(send.disabled, 'the second question is still open');
  click(options(root, 1)[0]);
  click(options(root, 1)[2]);
  await settle();
  equal(options(root, 0)[1].getAttribute('aria-checked'), 'true');
  equal(options(root, 1)[0].getAttribute('role'), 'checkbox', 'several can be chosen');
  equal(send.disabled, false);
  click(send);
  await settle();
  deepEqual(calls, [['question', 'c1', 't1', 'r1', { 'Which stack?': 'Go', 'Which extras?': 'Docs, CI' }]]);
}));

check('question card: writing another answer replaces the chosen option of a single choice, and Skip declines', () => withCard(async (root, calls) => {
  click(options(root, 0)[0]);
  const other = root.querySelector<HTMLInputElement>('fieldset.q .q-other')!;
  other.value = 'Zig';
  other.dispatchEvent(new InputEvent('input', { bubbles: true }));
  await settle();
  equal(options(root, 0)[0].getAttribute('aria-checked'), 'false', 'the tapped option was replaced');
  click(options(root, 1)[1]);
  await settle();
  const buttons = [...root.querySelectorAll<HTMLButtonElement>('.ask > div .btn')];
  click(buttons.find((b) => b.textContent?.trim() === 'Send answers'));
  await settle();
  deepEqual(calls[0], ['question', 'c1', 't1', 'r1', { 'Which stack?': 'Zig', 'Which extras?': 'Tests' }]);
}));

check('question card: Skip declines the question without sending answers', () => withCard(async (root, calls) => {
  click([...root.querySelectorAll<HTMLButtonElement>('.ask > div .btn')].find((b) => b.textContent?.trim() === 'Skip'));
  await settle();
  deepEqual(calls, [['answer', 'c1', 't1', 'r1', false]]);
}));

check('question card: tapping an option of a single choice clears what was written, so only one answer goes back', () => withCard(async (root, calls) => {
  const other = root.querySelector<HTMLInputElement>('fieldset.q .q-other')!;
  other.value = 'Zig';
  other.dispatchEvent(new InputEvent('input', { bubbles: true }));
  await settle();
  click(options(root, 0)[1]);
  click(options(root, 1)[0]);
  await settle();
  equal(root.querySelector<HTMLInputElement>('fieldset.q .q-other')!.value, '', 'the written answer was cleared');
  click([...root.querySelectorAll<HTMLButtonElement>('.ask > div .btn')].find((b) => b.textContent?.trim() === 'Send answers'));
  await settle();
  deepEqual(calls[0], ['question', 'c1', 't1', 'r1', { 'Which stack?': 'Go', 'Which extras?': 'Docs' }]);
}));

check('question card: a question whose text is a built-in name still renders and can be answered', () => withCard(async (root, calls) => {
  equal(root.querySelectorAll('fieldset.q').length, 2);
  click(options(root, 0)[0]);
  click(options(root, 1)[0]);
  await settle();
  click([...root.querySelectorAll<HTMLButtonElement>('.ask > div .btn')].find((b) => b.textContent?.trim() === 'Send answers'));
  await settle();
  deepEqual(calls[0], ['question', 'c1', 't1', 'r1', { constructor: 'Yes', toString: 'No' }]);
}, { questions: [{ question: 'constructor', options: [{ label: 'Yes' }] }, { question: 'toString', options: [{ label: 'No' }] }] }));
