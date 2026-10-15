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
