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
    // "Change" sends nothing: it opens the question in full with every choice, Faber's own picked and marked.
    const row = card[0]!.querySelectorAll('li')[1]!;
    const change = row.querySelector<HTMLButtonElement>('.change')!;
    change.click();
    await settle();
    deepEqual(sent, [], 'opening the choices sends nothing');
    equal(change.getAttribute('aria-expanded'), 'true');
    const choices = [...row.querySelectorAll<HTMLInputElement>('.answer-choices input')];
    deepEqual(choices.map((c) => c.closest('label')!.textContent), [`Yes · ${t('answers.chosen')}`, 'Later']);
    ok(choices[0]!.checked, "Faber's choice starts picked");
    const confirm = row.querySelector<HTMLButtonElement>('.answer-actions .send')!;
    ok(confirm.disabled, 'nothing to send until another choice is picked');
    // Cancel closes it with nothing sent; Escape does the same.
    row.querySelector<HTMLButtonElement>('.answer-actions .ghost')!.click();
    await settle();
    equal(row.querySelector('.answer-choices'), null);
    equal(document.activeElement, change, 'Cancel gives the focus back to Change');
    change.click();
    await settle();
    row.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();
    equal(row.querySelector('.answer-choices'), null, 'Escape closes the choices');
    deepEqual(sent, []);
    // Picking another choice and confirming sends it, as the person's message, and closes the choices.
    change.click();
    await settle();
    const later = row.querySelectorAll<HTMLInputElement>('.answer-choices input')[1]!;
    later.checked = true;
    later.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    deepEqual(sent, [], 'choosing another option only changes the pending choice');
    row.querySelector<HTMLButtonElement>('.answer-actions .send')!.click();
    await settle();
    deepEqual(sent, [t('answers.changeTo', { question: 'Check it on a phone?', answer: 'Later' })]);
    equal(row.querySelector('.answer-choices'), null);
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

check('run: trailing wake turns with only thinking keep the last reply with content open below the header', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const trailing: Message[] = [
    ...messages,
    { id: 'w6', role: 'user', text: 'Keep going on your own.', at: 1792007200000, wake: { why: 'wake.keepGoing' } },
    { id: 'a6', role: 'assistant', text: '', thinking: '**Reviewing stopped question status**', at: 1792007201000, finishedAt: 1792007205000, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5' },
  ];
  const dispose = render(() => createComponent(Thread, { conversationId: long.id, messages: trailing, from: 0, options: [], onBranch: () => {}, plan }), root);
  try {
    await settle();
    const shown = [...root.querySelectorAll(':scope > .ai')];
    equal(shown.length, 2, 'only the first reply and the last reply with content are shown');
    ok(shown[1]!.querySelector(':scope > .md')!.textContent!.includes('All the pages work.'), 'the conclusion stays visible');
    ok(shown[1]!.querySelector('.plan'), 'the plan stays attached to the visible reply');
  } finally {
    dispose();
    root.remove();
  }
});
