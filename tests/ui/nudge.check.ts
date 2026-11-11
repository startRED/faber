import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import type { Choice, Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import type { Option } from '../../ui/models.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const question: Message = { id: 'q', role: 'user', text: 'Fix the parser', at: 0 };
const answer = (extra: Partial<Message>): Message => ({ id: 'a', role: 'assistant', text: 'Done.', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5', effort: 'high', ...extra });

function mount(messages: Message[], options: Option[], onBranch: (id: string, text: string, choice?: Choice) => void) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'nudge-check', options, onBranch }), root);
  return { root, close: () => { dispose(); root.remove(); } };
}

check('nudge: the step-up button shows only under an answer with a nudge, and redoes the message on that rung', async () => {
  const nudge = { reason: 'nudge.failedCheck', vendor: 'claude', model: 'claude-opus-5-5', effort: 'medium' } as const;
  const calls: Array<[string, string, Choice | undefined]> = [];
  const options: Option[] = [{ label: 'Claude Opus', choice: { vendor: 'claude', model: 'opus' } }, { label: 'Claude Sonnet', choice: { vendor: 'claude', model: 'sonnet' } }];

  const plain = mount([question, answer({})], options, () => {});
  try {
    equal(plain.root.querySelector('.nudge'), null);
  } finally {
    plain.close();
  }

  const offered = mount([question, answer({ nudge })], options, (id, text, choice) => { calls.push([id, text, choice]); });
  try {
    const buttons = offered.root.querySelectorAll<HTMLButtonElement>('.nudge');
    equal(buttons.length, 1);
    equal(buttons[0]!.textContent, t('answer.nudge'));
    ok(buttons[0]!.title.includes('Opus 5.5'), `the tooltip names the rung: ${buttons[0]!.title}`);
    buttons[0]!.click();
    // The picker names Claude models by alias, so the redo carries the alias of the entry the nudge maps to.
    deepEqual(calls, [['q', 'Fix the parser', { vendor: 'claude', model: 'opus', effort: 'medium' }]]);
  } finally {
    offered.close();
  }

  calls.length = 0;
  const bare = mount([question, answer({ nudge })], [], (id, text, choice) => { calls.push([id, text, choice]); });
  try {
    bare.root.querySelector<HTMLButtonElement>('.nudge')!.click();
    deepEqual(calls, [['q', 'Fix the parser', { vendor: 'claude', model: 'claude-opus-5-5', effort: 'medium' }]], 'with no matching entry the nudge\'s own model is used');
  } finally {
    bare.close();
  }

  const streaming = mount([question, answer({ nudge, status: 'streaming' })], options, () => {});
  try {
    equal(streaming.root.querySelector('.nudge'), null, 'nothing is offered while the answer is still being written');
  } finally {
    streaming.close();
  }
});

// The conversation's box lives outside the thread; a bare one stands in for it.
function mountWithBox(messages: Message[], onBranch: (id: string, text: string, choice?: Choice) => void, onReply?: (text: string) => void) {
  const root = document.createElement('div');
  const composer = document.createElement('div');
  composer.className = 'composer';
  const box = document.createElement('textarea');
  composer.append(box);
  document.body.append(root, composer);
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'self-check', options: [], onBranch, onReply }), root);
  return { root, box, close: () => { dispose(); root.remove(); composer.remove(); } };
}

check('nudge: code nothing checked first offers to ask the same model to check it, and only starts the box', async () => {
  const labels = (root: HTMLElement) => [...root.querySelectorAll<HTMLButtonElement>('.actions .nudge')].map((b) => b.textContent);
  const sent: string[] = [];
  const redone: string[] = [];

  const offered = mountWithBox([question, answer({ selfCheck: true })], (id) => { redone.push(id); }, (text) => sent.push(text));
  try {
    const buttons = offered.root.querySelectorAll<HTMLButtonElement>('.actions .nudge');
    deepEqual(labels(offered.root), [t('answer.check')], 'the stronger retry is not offered beside it');
    equal(buttons[0]!.title, t('nudge.selfCheck'), 'the tooltip says why and what it does');
    buttons[0]!.click();
    equal(offered.box.value, t('nudge.selfCheck.ask'), 'the box holds the request');
    buttons[0]!.click();
    equal(offered.box.value, t('nudge.selfCheck.ask'), 'a second click does not write it twice');
    deepEqual([sent, redone], [[], []], 'nothing is sent or run again until the person sends it');
  } finally {
    offered.close();
  }

  const older = mountWithBox([question, answer({ selfCheck: true }), { ...question, id: 'q2' }, answer({ id: 'b' })], () => {}, () => {});
  try {
    deepEqual(labels(older.root), [], 'only the newest answer offers it');
  } finally {
    older.close();
  }

  const readOnly = mountWithBox([question, answer({ selfCheck: true })], () => {});
  try {
    deepEqual(labels(readOnly.root), [], 'a conversation that takes no reply offers nothing');
  } finally {
    readOnly.close();
  }

  // The answer after the ask is still unchecked: the engine names a rung, and the stronger retry is back.
  const still = mountWithBox([question, answer({ nudge: { reason: 'nudge.noCheck', vendor: 'claude', model: 'claude-opus-5-5', effort: 'medium' } })], () => {}, () => {});
  try {
    deepEqual(labels(still.root), [t('answer.nudge')]);
  } finally {
    still.close();
  }
});
