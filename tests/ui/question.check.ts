import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Permission, type RunningItem } from '../../ui/api.ts';
import { askedQuestions, Asks } from '../../ui/Ask.tsx';
import { useLanguage } from '../../ui/i18n.ts';
import { Running } from '../../ui/Running.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

// The assistant's own questions: a card in its message, one step per question, a tile per answer, answered by tapping
// (docs/product-map.md, "Question card").

const settle = (ms = 20) => new Promise((resolve) => setTimeout(resolve, ms));

const INPUT = {
  questions: [
    { question: 'Which stack?', header: 'Stack', multiSelect: false, options: [{ label: 'Rust (Recommended)', description: 'Fast' }, { label: 'Go', description: 'Simple' }] },
    { question: 'Which extras?', header: 'Extras', multiSelect: true, options: [{ label: 'Docs' }, { label: 'Tests' }, { label: 'CI' }] },
  ],
};

async function withCard(body: (root: HTMLElement, calls: unknown[][], waiting: (list: Permission[]) => void) => Promise<void>, input: unknown = INPUT) {
  const originals = { ...api };
  useLanguage('en');
  const calls: unknown[][] = [];
  api.answerQuestion = async (...args) => void calls.push(['question', ...args]);
  api.answer = async (...args) => void calls.push(['answer', ...args]);
  const root = document.createElement('div');
  document.body.append(root);
  const [list, setList] = createSignal<Permission[]>([{ requestId: 'r1', tool: 'AskUserQuestion', input }]);
  const dispose = render(() => createComponent(Asks, { conversationId: 'c1', turnId: 't1', get permissions() { return list(); } }), root);
  try {
    await settle();
    await body(root, calls, setList);
  } finally {
    dispose();
    root.remove();
    Object.assign(api, originals);
  }
}

const click = (el: Element | null | undefined) => (el as HTMLElement).click();
const panel = (root: HTMLElement, question: number) => root.querySelectorAll<HTMLElement>('.q')[question];
const options = (root: HTMLElement, question: number) => [...panel(root, question).querySelectorAll<HTMLElement>('.q-opt')];
const label = (o: HTMLElement) => o.querySelector('.q-label')?.firstChild?.textContent;
const button = (root: HTMLElement, text: string) => [...root.querySelectorAll<HTMLButtonElement>('.q-foot .btn')].find((b) => b.textContent?.trim() === text)!;
const tabs = (root: HTMLElement) => [...root.querySelectorAll<HTMLElement>('[role="tab"]')];
const write = (el: HTMLInputElement, text: string) => {
  el.value = text;
  el.dispatchEvent(new InputEvent('input', { bubbles: true }));
};

check('question card: a malformed question list shows nothing to answer', () => {
  deepEqual(askedQuestions(undefined), []);
  deepEqual(askedQuestions({ questions: 'no' }), []);
  deepEqual(askedQuestions({ questions: [{ options: [] }, { question: 'Ok?', options: [{ label: 'Yes' }, { nope: 1 }] }] }).map((q) => q.options.length), [1]);
  deepEqual(askedQuestions({ questions: [{ question: 'Ok?', options: [{ label: 'A', preview: '  ' }, { label: 'B', preview: '[ ok ]' }, { label: 'C', preview: 4 }] }] })[0].options.map((o) => o.preview), [undefined, '[ ok ]', undefined]);
});

check('question card: each question is a step, the open one shows and Send waits for every answer', () => withCard(async (root, calls) => {
  ok(root.querySelector('[data-testid="question-card"]'), 'a question card, not a permission card');
  equal(tabs(root).map((s) => s.querySelector('.q-t')?.textContent).join(), 'Stack,Extras');
  equal(tabs(root).map((s) => s.getAttribute('aria-selected')).join(), 'true,false');
  equal(panel(root, 0).hasAttribute('inert'), false);
  ok(panel(root, 1).hasAttribute('inert'), 'the other step waits, out of reach');
  equal(root.querySelector('.q-sum')?.textContent, '0 of 2 answered');
  equal(options(root, 0).map(label).join(), 'Rust,Go');
  const send = button(root, 'Send answers');
  ok(send.disabled, 'nothing chosen yet');
  click(options(root, 0)[1]);
  await settle();
  ok(send.disabled, 'the second question is still open');
  ok(tabs(root)[0].classList.contains('done'), 'a done step is ticked');
  equal(root.querySelector('.q-sum')?.textContent, '1 of 2 answered');
  click(options(root, 1)[0]);
  click(options(root, 1)[2]);
  await settle();
  equal(options(root, 0)[1].getAttribute('aria-checked'), 'true');
  equal(options(root, 0)[1].getAttribute('role'), 'radio');
  equal(panel(root, 0).querySelector('[role="radiogroup"]') !== null, true);
  equal(options(root, 1)[0].getAttribute('role'), 'checkbox', 'several can be chosen');
  equal(send.disabled, false);
  click(send);
  await settle();
  deepEqual(calls, [['question', 'c1', 't1', 'r1', { 'Which stack?': 'Go', 'Which extras?': 'Docs, CI' }]]);
}));

check('question card: choosing a single answer moves on to the next open question, and the steps can be walked with the arrows', () => withCard(async (root) => {
  click(options(root, 0)[0]);
  await settle(320);
  equal(tabs(root)[1].getAttribute('aria-selected'), 'true', 'it moved to the next question by itself');
  equal(panel(root, 1).hasAttribute('inert'), false);
  ok(panel(root, 0).hasAttribute('inert'));
  tabs(root)[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
  await settle();
  equal(tabs(root)[0].getAttribute('aria-selected'), 'true');
  equal(tabs(root)[0].getAttribute('tabindex'), '0');
  equal(tabs(root)[1].getAttribute('tabindex'), '-1');
}));

check('question card: the arrows move through the tiles of a single choice and take the one they land on', () => withCard(async (root) => {
  const [rust, go] = options(root, 0);
  equal(rust.getAttribute('tabindex'), '0', 'one tile is in the tab order');
  equal(go.getAttribute('tabindex'), '-1');
  rust.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
  await settle();
  equal(go.getAttribute('aria-checked'), 'true');
  equal(go.getAttribute('tabindex'), '0');
  equal(rust.getAttribute('tabindex'), '-1');
}));

check('question card: "(Recommended)" is a badge on the tile, and the label goes back as the assistant wrote it', () => withCard(async (root, calls) => {
  const rust = options(root, 0)[0];
  equal(label(rust), 'Rust');
  equal(rust.querySelector('.q-rec')?.textContent, 'Recommended');
  equal(options(root, 0)[1].querySelector('.q-rec'), null);
  click(rust);
  click(options(root, 1)[0]);
  await settle();
  click(button(root, 'Send answers'));
  await settle();
  deepEqual(calls[0], ['question', 'c1', 't1', 'r1', { 'Which stack?': 'Rust (Recommended)', 'Which extras?': 'Docs' }]);
}));

check('question card: another answer is one quiet button that opens the field, and it replaces the chosen tile of a single choice', () => withCard(async (root, calls) => {
  const more = panel(root, 0).querySelector<HTMLButtonElement>('.q-more')!;
  equal(more.getAttribute('aria-expanded'), 'false');
  equal(more.textContent?.trim(), 'Write another answer');
  const fold = panel(root, 0).querySelector<HTMLElement>('.fold')!;
  ok(fold.hasAttribute('inert'), 'the field is closed until asked for');
  click(options(root, 0)[0]);
  click(more);
  await settle();
  equal(more.getAttribute('aria-expanded'), 'true');
  equal(fold.hasAttribute('inert'), false);
  const field = fold.querySelector<HTMLInputElement>('input')!;
  equal(document.activeElement, field, 'the field takes the focus');
  write(field, 'Zig');
  await settle();
  equal(options(root, 0)[0].getAttribute('aria-checked'), 'false', 'the tapped tile was replaced');
  click(more);
  await settle();
  equal(more.textContent?.trim(), 'Zig', 'closed, the button shows what was written');
  click(options(root, 1)[1]);
  await settle();
  click(button(root, 'Send answers'));
  await settle();
  deepEqual(calls[0], ['question', 'c1', 't1', 'r1', { 'Which stack?': 'Zig', 'Which extras?': 'Tests' }]);
}));

check('question card: Skip declines the question without sending answers', () => withCard(async (root, calls) => {
  click(button(root, 'Skip'));
  await settle();
  deepEqual(calls, [['answer', 'c1', 't1', 'r1', false]]);
}));

check('question card: tapping a tile of a single choice clears what was written, so only one answer goes back', () => withCard(async (root, calls) => {
  click(panel(root, 0).querySelector('.q-more'));
  write(panel(root, 0).querySelector<HTMLInputElement>('input')!, 'Zig');
  await settle();
  click(options(root, 0)[1]);
  click(options(root, 1)[0]);
  await settle();
  equal(panel(root, 0).querySelector<HTMLInputElement>('input')!.value, '', 'the written answer was cleared');
  click(button(root, 'Send answers'));
  await settle();
  deepEqual(calls[0], ['question', 'c1', 't1', 'r1', { 'Which stack?': 'Go', 'Which extras?': 'Docs' }]);
}));

check('question card: a single question has no steps, and a tile with a drawing shows it beside the tile in focus', () => withCard(async (root, calls) => {
  equal(root.querySelector('[role="tablist"]'), null, 'one question needs no steps');
  equal(root.querySelector('.q-sum'), null);
  equal(root.querySelector('.q-head')?.textContent, 'Layout');
  const preview = () => root.querySelector('.q-preview');
  equal(preview()?.querySelector('pre')?.textContent, '[ side ]', 'the first drawing shows before anything is chosen, fences removed');
  const [side, , plain] = options(root, 0);
  options(root, 0)[1].dispatchEvent(new MouseEvent('mouseenter'));
  await settle();
  equal(preview()?.querySelector('pre')?.textContent, '[ top ]', 'the drawing follows the tile in focus');
  equal(preview()?.getAttribute('aria-label'), 'Preview of Top');
  plain.dispatchEvent(new FocusEvent('focus'));
  await settle();
  equal(preview()?.querySelector('pre'), null);
  equal(preview()?.textContent, 'This option has no preview.');
  click(side);
  await settle();
  click(button(root, 'Send answers'));
  await settle();
  deepEqual(calls[0], ['question', 'c1', 't1', 'r1', { 'Where?': 'Side' }]);
}, {
  questions: [{ question: 'Where?', header: 'Layout', options: [{ label: 'Side', preview: '```\n[ side ]\n```' }, { label: 'Top', preview: '[ top ]' }, { label: 'None' }] }],
}));

const BIGGER = { questions: [{ question: 'Keep it free of crashes?', options: [{ label: 'Fix this crash' }, { label: 'Keep the app free of crashes' }] }] };

check('question card: the bigger ask starts on the request as asked and one press of Send sends it', () => withCard(async (root, calls) => {
  equal(options(root, 0)[0].getAttribute('aria-checked'), 'true', 'the first tile is already chosen');
  equal(options(root, 0)[1].getAttribute('aria-checked'), 'false');
  const send = button(root, 'Send answers');
  equal(send.disabled, false);
  click(send);
  await settle();
  deepEqual(calls, [['question', 'c1', 't1', 'r1', { 'Keep it free of crashes?': 'Fix this crash' }]]);
}, { bigger: true, ...BIGGER }));

check('question card: the same words without the mark of the engine start with nothing chosen', () => withCard(async (root, calls) => {
  equal(options(root, 0)[0].getAttribute('aria-checked'), 'false');
  ok(button(root, 'Send answers').disabled, 'nothing chosen yet');
  deepEqual(calls, []);
}, BIGGER));

check('question card: questions without a drawing have no preview panel', () => withCard(async (root) => {
  equal(root.querySelector('.q-preview'), null);
  equal(root.querySelector('.with-preview'), null);
}));

check('question card: an answered card folds away before it leaves, and the other waiting questions are a note', () => withCard(async (root, _calls, waiting) => {
  const other = { requestId: 'r2', tool: 'AskUserQuestion', input: INPUT };
  waiting([{ requestId: 'r1', tool: 'AskUserQuestion', input: INPUT }, other]);
  await settle();
  equal(root.querySelector('.ask-more')?.textContent, '1 more waiting');
  waiting([]);
  await settle(20);
  ok(root.querySelector('.ask.question.gone'), 'the card folds while it still exists');
  equal(root.querySelector('.ask.question.gone .q-inner')?.hasAttribute('inert'), true, 'and can no longer be used');
  await settle(400);
  equal(root.querySelector('.ask.question'), null, 'then it is gone');
}));

check('question card: a question whose text is a built-in name still renders and can be answered', () => withCard(async (root, calls) => {
  equal(root.querySelectorAll('.q').length, 2);
  click(options(root, 0)[0]);
  click(options(root, 1)[0]);
  await settle();
  click(button(root, 'Send answers'));
  await settle();
  deepEqual(calls[0], ['question', 'c1', 't1', 'r1', { constructor: 'Yes', toString: 'No' }]);
}, { questions: [{ question: 'constructor', options: [{ label: 'Yes' }] }, { question: 'toString', options: [{ label: 'No' }] }] }));

check('question card: in the running column an answered card folds away once, still showing its question, however often the list is rebuilt', async () => {
  useLanguage('en');
  const root = document.createElement('div');
  document.body.append(root);
  const row = (permissions: Permission[]): RunningItem => ({
    conversationId: 'c1', turnId: 't1', title: 'Pick a stack', area: 'chat', vendor: 'claude', state: 'working', since: Date.now(), permissions,
  });
  const [items, setItems] = createSignal<RunningItem[]>([row([{ requestId: 'r1', tool: 'AskUserQuestion', input: INPUT }])]);
  const dispose = render(() => createComponent(Running, {
    get items() { return items(); }, projects: [], onOpen: () => {}, onClose: () => {}, modal: false, ref: () => {},
  }), root);
  try {
    await settle();
    ok(root.querySelector('.ask.question'), 'the question waits in the column');
    const card = root.querySelector('.ask.question');
    const line = root.querySelector('li.ri');
    // The engine sends a new list (new objects) at every change: the answer, then two more inside the fold.
    setItems([row([])]);
    await settle(20);
    ok(root.querySelector('.ask.question.gone'), 'the card folds while it still exists');
    equal(root.querySelector('.ask.question.gone .q-text')?.textContent, 'Which stack?', 'and still shows its question');
    setItems([{ ...row([]), now: 'Reading files' }]);
    await settle(30);
    setItems([{ ...row([]), now: 'Writing tests' }]);
    await settle(30);
    equal(root.querySelector('.ask.question'), card, 'the same card, so its fold ran once and was not started over');
    equal(root.querySelector('li.ri'), line, 'in the same row');
    ok(root.querySelector('.ask.question.gone'), 'still folding');
    equal(document.querySelector('.motion-exit'), null, 'and no copy of an old row fades beside it');
    await settle(400);
    equal(root.querySelector('.ask.question'), null, 'then it is gone');
    equal(root.querySelector('li.ri .run-kg .last')?.textContent, 'Writing tests', 'the row went on showing the latest list');
  } finally {
    dispose();
    root.remove();
  }
});
