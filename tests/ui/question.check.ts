import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Permission, type RunningItem } from '../../ui/api.ts';
import { askedQuestions, Asks } from '../../ui/Ask.tsx';
import { useLanguage } from '../../ui/i18n.ts';
import { Running } from '../../ui/Running.tsx';
import { appliedLook, applyLook, motion } from '../../ui/appearance.ts';
import { millis } from '../../ui/motion.ts';
import { keepRoot } from './appearance.check.ts';
import { check, deepEqual, equal, finished, ok } from './check.ts';

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

check('question card: failed delivery preserves answers for one unchanged retry', () => withCard(async (root, calls) => {
  const pending: { resolve: () => void; reject: (reason: unknown) => void }[] = [];
  api.answerQuestion = (...args) => {
    calls.push(['question', ...args]);
    return new Promise<void>((resolve, reject) => pending.push({ resolve, reject }));
  };
  click(options(root, 0)[1]);
  click(tabs(root)[1]);
  click(panel(root, 1).querySelector('.q-more'));
  const field = panel(root, 1).querySelector<HTMLInputElement>('input')!;
  write(field, 'Backups');
  await settle();
  const send = button(root, 'Send answers');
  const inner = root.querySelector<HTMLElement>('.q-inner')!;
  click(send);
  await settle();
  const expected = ['question', 'c1', 't1', 'r1', { 'Which stack?': 'Go', 'Which extras?': 'Backups' }];
  deepEqual(calls, [expected]);
  ok(inner.inert, 'the card waits for delivery before accepting more input');
  equal(inner.getAttribute('aria-busy'), 'true');
  pending[0].reject('error.permission.undelivered');
  await settle();
  ok(root.querySelector('.note.err[role="alert"]')?.textContent?.includes('Try again.'), 'failed delivery has an actionable error');
  equal(options(root, 0)[1].getAttribute('aria-checked'), 'true');
  equal(field.value, 'Backups', 'the written answer survives delivery failure');
  ok(!inner.inert && !send.disabled, 'failed delivery releases the card for retry');
  equal(inner.getAttribute('aria-busy'), 'false');
  click(send);
  await settle();
  deepEqual(calls, [expected, expected], 'the retry sends the same answers exactly once');
  equal(root.querySelector('.note.err'), null, 'retry clears the previous error');
  ok(inner.inert, 'retry also waits for delivery');
  pending[1].resolve();
  await settle();
  equal(root.querySelector('.note.err'), null);
  ok(!inner.inert && !send.disabled, 'completion releases the card');
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

check('question card: a finished entrance leaves the answered card free to fade out', () => withCard(async (root, _calls, waiting) => {
  const card = root.querySelector<HTMLElement>('.ask.question')!;
  const arrival = card.getAnimations().find((animation) =>
    (animation as CSSAnimation).animationName === 'arrive' ||
    animation instanceof CSSTransition && animation.transitionProperty === 'opacity');
  ok(arrival, 'the full-motion card has its real entrance');
  await finished([arrival!]);
  equal(arrival!.playState, 'finished', 'the actual entrance finishes before answering');
  equal(Number(getComputedStyle(card).opacity), 1, 'the entrance has settled visibly');
  waiting([]);
  ok(card.isConnected && card.classList.contains('gone'), 'the same card stays while leaving');
  ok(card.querySelector('.q-inner')?.hasAttribute('inert'), 'the answered card takes no input');
  const exit = card.getAnimations().find((animation) => animation instanceof CSSTransition && animation.transitionProperty === 'opacity');
  const style = getComputedStyle(card);
  const diagnostic = { opacity: style.opacity, animation: style.animationName, fill: style.animationFillMode,
    transition: style.transitionProperty, duration: style.transitionDuration, motion: document.documentElement.dataset.motion,
    reduced: matchMedia('(prefers-reduced-motion: reduce)').matches,
    animations: card.getAnimations().map((animation) => ({ kind: animation.constructor.name, state: animation.playState,
      property: animation instanceof CSSTransition ? animation.transitionProperty : undefined })) };
  ok(exit, `the completed entrance must allow the real opacity exit transition: ${JSON.stringify(diagnostic)}`);
  const duration = Number(exit!.effect!.getComputedTiming().duration);
  ok(duration > 0, 'the opacity exit has a positive duration');
  exit!.pause();
  exit!.currentTime = duration / 2;
  const opacity = Number(getComputedStyle(card).opacity);
  ok(opacity > 0 && opacity < 1, `the connected exit passes through partial opacity, got ${opacity}`);
  exit!.finish();
  await settle(180);
  equal(root.querySelector('.ask.question'), null, 'the answered card is eventually removed');
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

check('question card: answering from Running during partial thread arrival starts a real opacity exit', async () => {
  useLanguage('en');
  const threadRoot = document.createElement('div');
  const runningRoot = document.createElement('div');
  const permission: Permission = { requestId: 'r1', tool: 'AskUserQuestion', input: {
    questions: [{ question: 'Which stack?', options: [{ label: 'Rust' }] },
  ] } };
  const [permissions, setPermissions] = createSignal<Permission[]>([permission]);
  const originals = { answerQuestion: api.answerQuestion };
  const calls: unknown[][] = [];
  let disposeThread: (() => void) | undefined;
  let disposeRunning: (() => void) | undefined;
  const nextFrameBefore = (deadline: number) => new Promise<boolean>((resolve) => {
    const remaining = deadline - performance.now();
    if (remaining <= 0) return resolve(false);
    let done = false;
    let frame = 0;
    const timer = setTimeout(() => finish(false), remaining);
    const finish = (advanced: boolean) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      if (frame) cancelAnimationFrame(frame);
      resolve(advanced);
    };
    try {
      frame = requestAnimationFrame(() => finish(true));
    } catch {
      finish(false);
    }
  });
  try {
    document.body.append(threadRoot, runningRoot);
    api.answerQuestion = async (...args) => {
      calls.push(args);
      // The shared pending state changes only when the real Running control calls the answer API.
      setPermissions([]);
    };
    let constructionFailed = false;
    let constructionError: unknown;
    // Catch inside the owned callback so render returns its disposer even when construction fails.
    disposeThread = render(() => {
      try {
        return createComponent(Asks, {
          conversationId: 'c1', turnId: 't1', get permissions() { return permissions(); },
        });
      } catch (error) {
        constructionFailed = true;
        constructionError = error;
      }
    }, threadRoot);
    if (constructionFailed) throw constructionError;
    const item = (): RunningItem => ({
      conversationId: 'c1', turnId: 't1', title: 'Pick a stack', area: 'chat', vendor: 'claude',
      state: 'working', since: Date.now(), permissions: permissions(),
    });
    // Running reads the same signal directly; no detached effect or mirrored state is needed.
    disposeRunning = render(() => {
      try {
        return createComponent(Running, {
          get items() { return [item()]; }, projects: [], onOpen: () => {}, onClose: () => {}, modal: false, ref: () => {},
        });
      } catch (error) {
        constructionFailed = true;
        constructionError = error;
      }
    }, runningRoot);
    if (constructionFailed) throw constructionError;

    const threadCard = threadRoot.querySelector<HTMLElement>('.ask.question')!;
    const runningCard = runningRoot.querySelector<HTMLElement>('.ask.question')!;
    ok(threadCard && runningCard, 'both real Asks surfaces show the same pending question');
    const arrival = threadCard.getAnimations().find((a) =>
      (a as CSSAnimation).animationName === 'arrive' ||
      a instanceof CSSTransition && a.transitionProperty === 'opacity')!;
    ok(arrival, 'the Thread copy has its real entrance animation');

    // The independent timer bounds this wait even if the hidden document stops delivering animation frames.
    // A missed natural overlap is a hard precondition failure, never a passing exit result.
    const deadline = performance.now() + 1200;
    let before: { duration: number } | undefined;
    while (performance.now() < deadline && await nextFrameBefore(deadline)) {
      const duration = Number(arrival.effect?.getComputedTiming().duration);
      const opacity = Number(getComputedStyle(threadCard).opacity);
      if (arrival.playState === 'running' && Number(arrival.currentTime) > 0 &&
          Number(arrival.currentTime) < duration && opacity > 0 && opacity < 1) {
        before = { duration };
        break;
      }
    }
    ok(before, 'PRECONDITION FAILED: no natural partial-opacity Thread entrance frame within 1200 ms');

    const option = runningCard.querySelector<HTMLElement>('.q-opt')!;
    option.click();
    const send = [...runningCard.querySelectorAll<HTMLButtonElement>('.q-foot .btn')]
      .find((button) => button.textContent?.trim() === 'Send answers')!;
    ok(send && !send.disabled, 'the actual Running copy has an enabled Send answers control');
    deepEqual(permissions().map((p) => p.requestId), ['r1']);
    const preClickOpacity = Number(getComputedStyle(threadCard).opacity);
    const preClickTime = Number(arrival.currentTime);
    ok(arrival.playState === 'running' && preClickTime < before!.duration &&
      preClickOpacity > 0 && preClickOpacity < 1,
      'PRECONDITION FAILED: Thread opacity is no longer partial immediately before the real click');

    // The stub clears the shared signal synchronously during this real click. Inspect immediately so the
    // short transition cannot be hidden by a settle delay.
    const answeredAt = performance.now();
    send.click();
    deepEqual(calls, [['c1', 't1', 'r1', { 'Which stack?': 'Rust' }]]);
    ok(threadCard.isConnected && threadCard.classList.contains('gone'), 'the same Thread card enters its gone state');
    ok(threadCard.querySelector('.q-inner')?.hasAttribute('inert'), 'the retained Thread card is inert');
    const exit = threadCard.getAnimations().find((a) => a instanceof CSSTransition && a.transitionProperty === 'opacity');
    const snapshots: unknown[] = [];
    const snapshot = () => ({
        at: performance.now(), connected: threadCard.isConnected,
        opacity: getComputedStyle(threadCard).opacity, transform: getComputedStyle(threadCard).transform,
        animation: getComputedStyle(threadCard).animation, transition: getComputedStyle(threadCard).transition,
        animations: threadCard.getAnimations().map((a) => ({
          kind: a.constructor.name, name: (a as CSSAnimation).animationName,
          property: (a as CSSTransition).transitionProperty, state: a.playState, time: a.currentTime,
          timing: a.effect?.getComputedTiming(), keyframes: (a.effect as KeyframeEffect | null)?.getKeyframes(),
        })),
      });
    if (!exit) {
      snapshots.push(snapshot());
      const diagnosticDeadline = performance.now() + 100;
      for (let frame = 0; frame < 3 && await nextFrameBefore(diagnosticDeadline); frame++) snapshots.push(snapshot());
    }
    ok(exit, `a real opacity CSSTransition starts on the connected Thread card; diagnostic ${JSON.stringify({
      preClickOpacity, preClickTime, arrivalDuration: before!.duration, snapshots,
    })}`);
    const exitDuration = Number(exit!.effect?.getComputedTiming().duration);
    ok(exitDuration > 0, `the actual opacity exit has positive duration, got ${exitDuration}`);

    const initialOpacity = Number(getComputedStyle(threadCard).opacity);
    const continuous = Math.abs(initialOpacity - preClickOpacity) < 0.02;
    ok(continuous, `exit starts at the current opacity without a flash; diagnostic ${JSON.stringify({
      preClickOpacity, initialOpacity, preClickTime,
      exit: continuous ? undefined : {
        currentTime: exit!.currentTime, playState: exit!.playState,
        effectTargetsCard: (exit!.effect as KeyframeEffect | null)?.target === threadCard,
        timing: exit!.effect?.getComputedTiming(),
        endpoints: (exit!.effect as KeyframeEffect | null)?.getKeyframes()
          .filter((_, index, frames) => index === 0 || index === frames.length - 1)
          .map(({ opacity, easing }) => ({ opacity, easing })),
      },
    })}`);
    const endpoints = (exit!.effect as KeyframeEffect).getKeyframes();
    ok(Math.abs(Number(endpoints[0].opacity) - preClickOpacity) < 0.02, 'the real exit starts at the interrupted entrance');
    equal(Number(endpoints.at(-1)!.opacity), 0, 'the real exit targets zero opacity');
    ok(!threadCard.getAnimations().some((a) => a instanceof CSSAnimation && a.animationName === 'arrive'),
      'no arrival keyframes compete with the exit');
    const configuredExit = millis(document.documentElement, '--dur-exit', 180);
    const fold = threadCard.getAnimations().find((a) => a instanceof CSSTransition && a.transitionProperty === 'grid-template-rows');
    ok(fold, 'the card has its real layout fold');
    equal(Number(fold!.effect?.getComputedTiming().duration), configuredExit, 'the layout fold retains the full exit time');
    const removalAtDeadline = settle(Math.max(0, answeredAt + configuredExit + 100 - performance.now())).then(() => ({
      card: threadRoot.querySelector('.ask.question'), elapsed: performance.now() - answeredAt,
    }));
    let finishedNaturally = false;
    let cancelled = false;
    void Promise.all([exit!.finished, fold!.finished]).then(() => { finishedNaturally = true; }, () => { cancelled = true; });
    // A reversed opacity transition may finish between frames; require its endpoints and natural completion instead.
    const exitDeadline = answeredAt + configuredExit + 250;
    const exitFrames: unknown[] = [];
    const recordExit = (currentExit: Animation | undefined, opacity: number) => {
      exitFrames.push({
        elapsed: performance.now() - answeredAt, connected: threadCard.isConnected,
        gone: threadCard.classList.contains('gone'), opacity,
        time: currentExit?.currentTime, state: currentExit?.playState,
      });
    };
    recordExit(exit, Number(getComputedStyle(threadCard).opacity));
    let previousOpacity = initialOpacity;
    while (!finishedNaturally && !cancelled && performance.now() < exitDeadline && await nextFrameBefore(exitDeadline)) {
      const currentExit = threadCard.getAnimations().find((a) => a instanceof CSSTransition && a.transitionProperty === 'opacity');
      const opacity = Number(getComputedStyle(threadCard).opacity);
      recordExit(currentExit, opacity);
      ok(opacity >= 0 && opacity <= previousOpacity + 0.02, `the natural exit never rebounds; ${JSON.stringify(exitFrames)}`);
      previousOpacity = opacity;
    }
    ok(finishedNaturally && !cancelled, `the real opacity exit and layout fold finish naturally; diagnostic ${JSON.stringify({
      preClickOpacity, preClickTime, exitDuration, configuredExit, exitFrames,
      elapsed: performance.now() - answeredAt, connected: threadCard.isConnected,
    })}`);
    equal(Number(getComputedStyle(threadCard).opacity), 0, 'the naturally finished exit is invisible');
    const removal = await removalAtDeadline;
    equal(removal.card, null, `Thread removes its answered card by the configured fold deadline; diagnostic ${JSON.stringify({
      exitDuration, configuredExit, elapsed: removal.elapsed,
    })}`);
  } finally {
    try {
      disposeRunning?.();
    } finally {
      try {
        disposeThread?.();
      } finally {
        try {
          api.answerQuestion = originals.answerQuestion;
        } finally {
          runningRoot.remove();
          threadRoot.remove();
        }
      }
    }
  }
});

for (const level of ['reduced', 'none'] as const) {
  check(`question card: ${level} motion removes an answered card without the full-motion delay`, async () => {
    const saved = appliedLook();
    ok(saved, 'the app has applied its real look before this check');
    const restore = keepRoot();
    try {
      await applyLook({ ...saved, motion: level });
      equal(motion(), level, 'the reactive motion preference is in force');
      equal(document.documentElement.dataset.motion, level, 'the stylesheet uses the same preference');
      await withCard(async (root, _calls, waiting) => {
        ok(root.querySelector('.ask.question'), 'the pending question is present');
        waiting([]);
        await settle();
        equal(root.querySelector('.ask.question'), null, 'the answered card is removed without a full-motion exit delay');
      });
    } finally {
      try {
        await applyLook(saved);
      } finally {
        restore();
      }
    }
  });
}
