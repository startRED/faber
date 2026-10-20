import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import type { Message, Plan } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { folded, runsOf } from '../../ui/Run.tsx';
import { Thread } from '../../ui/Thread.tsx';
import long from './fixtures/long-run.json';
import { check, deepEqual, equal, ok } from './check.ts';

// Task 018: a Keep going run reads as one block. The recording is a person's request and five turns Faber sent by itself,
// one of them with nothing in it, one with a question Faber answered, the newest closed with a box Faber answered too.
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
const messages = long.messages as Message[];
const plan = long.plan as Plan;

check('run: turns Faber sent by itself are one run, and a message of the person ends it', () => {
  const runs = runsOf(messages);
  deepEqual([...runs.keys()], [messages.length - 1]);
  equal(runs.get(messages.length - 1)!.replies.length, 5);
  equal(folded(runs).size, 4, 'all but the newest fold');
  // One turn Faber sent reads as a reply like any other.
  const one = messages.slice(0, 4);
  equal(runsOf(one).size, 0);
  // The person writes in between: two runs, one on each side of it.
  const split = [...messages.slice(0, 6), { id: 'me', role: 'user', text: 'Wait', at: 1 } as Message, ...messages.slice(6)];
  deepEqual([...runsOf(split).values()].map((r) => r.replies.length), [2, 3]);
});

check('run: one header with the time, the turns and the plan, the earlier turns folded, the answers in one card', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const sent: string[] = [];
  const dispose = render(
    () => createComponent(Thread, { conversationId: long.id, messages, from: 0, options: [], onBranch: () => {}, plan, onReply: (text: string) => sent.push(text) }),
    root,
  );
  try {
    await settle();
    const head = root.querySelector('.run-block > summary')!;
    const title = t('run.block.title', { time: t('time.hours', { n: 1 }), turns: t('run.block.turns', { n: 5 }) });
    equal(head.textContent!.trim(), `${title} · ${t('run.block.plan', { count: t('plan.count', { done: 2, total: 3 }) })}`);
    ok(head.getAttribute('title')!.includes('×4'), 'the tooltip counts what Faber asked');
    // Only the person's own first reply and the newest turn are drawn; the earlier turns wait behind the header.
    equal(root.querySelectorAll('.ai').length, 2);
    equal(root.querySelector('.run-earlier'), null, 'the folded turns are not drawn until opened');
    (root.querySelector('.run-block') as HTMLDetailsElement).open = true;
    await settle();
    // Opened: the three earlier turns with something in them; the empty one is not drawn.
    equal(root.querySelectorAll('.run-earlier .ai').length, 3);
    equal(root.querySelector('.run-earlier .answers'), null, 'an earlier turn leaves its answers to the run card');
    // Both questions Faber answered are rows of the newest reply's card, with the answer it chose.
    const card = root.querySelectorAll('.answers');
    equal(card.length, 1);
    equal(card[0]!.querySelector('.woke')!.textContent!.trim(), t('answers.title', { n: 2 }));
    deepEqual(
      [...card[0]!.querySelectorAll('li')].map((li) => [li.querySelector('.q')!.textContent, li.querySelector('.a')!.textContent]),
      [['Which database for the cache?', 'SQLite'], ['Check it on a phone?', 'Yes']],
    );
    // "Change" with one other choice sends it at once, as the person's message.
    card[0]!.querySelectorAll<HTMLButtonElement>('.change')[1]!.click();
    deepEqual(sent, [t('answers.changeTo', { question: 'Check it on a phone?', answer: 'Later' })]);
    // The closing box Faber answered offers no buttons of its own.
    equal(root.querySelector('.wrap-actions'), null);
  } finally {
    dispose();
    root.remove();
  }
});

check('run: narration in the work log folds to its last two paragraphs, and the answer never folds', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Thread, { conversationId: long.id, messages: messages.slice(-2), from: 0, options: [], onBranch: () => {} }), root);
  try {
    await settle();
    const log = root.querySelector('.activity-body')!;
    const more = log.querySelector<HTMLButtonElement>('.more-narration')!;
    equal(more.textContent!.trim(), t('narration.more', { n: 1 }));
    ok(!log.textContent!.includes('First I read the page') && log.textContent!.includes('Then I checked the form'), log.textContent!);
    ok(root.querySelector(':scope .ai > .md')!.textContent!.includes('All the pages work.'), 'the answer stays whole');
    more.click();
    await settle();
    ok(log.textContent!.includes('First I read the page'));
    equal(log.querySelector('.more-narration'), null);
  } finally {
    dispose();
    root.remove();
  }
});

check('run: a newest turn with nothing of its own still holds the run\'s answers', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const quiet = messages.map((m, i) => (i === messages.length - 1 ? { ...m, text: '', tools: [], wrapUp: undefined, away: undefined, activityEnd: undefined } : m));
  const dispose = render(() => createComponent(Thread, { conversationId: long.id, messages: quiet, from: 0, options: [], onBranch: () => {} }), root);
  try {
    await settle();
    equal(root.querySelector('.answers .woke')!.textContent!.trim(), t('answers.title', { n: 1 }));
  } finally {
    dispose();
    root.remove();
  }
});
