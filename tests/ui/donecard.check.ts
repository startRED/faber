import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Summary } from '../../ui/api.ts';
import { lang, t, useLanguage } from '../../ui/i18n.ts';
import { Running } from '../../ui/Running.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

// Task 016 (the legible agent runs design): a run that ends while nobody looks keeps its place in the running column as a calm
// card; a click opens the conversation, its × only clears it.
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
const card = (id: string, kind: 'done' | 'needsYou' | 'failed', over: Partial<Summary> = {}): Summary => ({
  id,
  title: `Fix ${id}`,
  area: 'project',
  projectId: 'p1',
  updatedAt: 0,
  ended: { at: 1, kind, workMs: 2 * 3_600_000 + 600_000, line: 'All the pages load again.' },
  ...over,
});

check('done card: project follow-ups are at most three plain buttons, wait during start and retry failures', async () => {
  const next = { messageId: 'closed', items: ['Check the phone layout because it is still untested.', 'Document the new path because it moved.', 'Cover the empty case because it failed before.', 'Not shown'] };
  const done = card('follow-ups', 'done');
  done.ended!.next = next;
  const chat = { ...done, id: 'chat', area: 'chat' as const, projectId: undefined };
  const calls: [string, number][] = [];
  let finish!: () => void;
  let reject!: () => void;
  const root = document.createElement('div'); root.dataset.motion = 'none'; root.style.cssText = 'position:fixed;left:0;top:0;width:720px';
  document.body.append(root);
  const before = lang();
  const dispose = render(() => createComponent(Running, {
    items: [], done: [done, chat], projects: [{ id: 'p1', name: 'Shop', path: '/shop' }],
    onOpen: () => {}, onClose: () => {}, modal: false, ref: () => {},
    onFollowUp: (c, index) => {
      calls.push([c.id, index]);
      return new Promise<void>((resolve, fail) => { finish = resolve; reject = () => fail(new Error('start failed')); });
    },
  }), root);
  const buttons = () => [...root.querySelectorAll<HTMLButtonElement>('.run-next button')];
  try {
    useLanguage('de'); await settle();
    equal(buttons().length, 3, 'Chat never offers a project follow-up');
    deepEqual(buttons().map((button) => button.textContent), next.items.slice(0, 3));
    ok(buttons().every((button) => button.classList.contains('ghost')));
    buttons()[1].click(); await settle();
    deepEqual(calls, [['follow-ups', 1]], 'the saved closing-box index reaches the project start path');
    ok(buttons().every((button) => button.disabled));
    reject(); await settle();
    equal(root.querySelector('[role="alert"]')!.textContent, t('run.followFailed'));
    buttons()[1].click(); finish(); await settle();
    deepEqual(calls, [['follow-ups', 1], ['follow-ups', 1]]);
    equal(root.querySelector('[role="alert"]'), null);
    ok(buttons().every((button) => !button.disabled));
    ok(buttons().every((button) => button.scrollWidth <= button.clientWidth + 1), 'follow-up text wraps in a narrow column');
  } finally { useLanguage(before); dispose(); root.remove(); }
});

check('done card: a run that ended shows how it ended, opens with a click and goes with its ×', async () => {
  const [done, setDone] = createSignal<Summary[]>([card('login', 'done'), card('build', 'needsYou')]);
  const opened: string[] = [];
  const dismissed: string[] = [];
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Running, {
    items: [],
    get done() { return done(); },
    projects: [{ id: 'p1', name: 'Shop', path: '/shop' }],
    onOpen: () => {},
    onOpenDone: (c) => opened.push(c.id),
    onDismiss: (c) => dismissed.push(c.id),
    onClose: () => {},
    modal: false,
    ref: () => {},
  }), root);
  try {
    await settle();
    const cards = [...root.querySelectorAll<HTMLElement>('.ri.done')];
    equal(cards.length, 2);
    equal(root.querySelector('.rc-empty'), null, 'a done card is not "Nothing running"');
    equal(root.querySelector('.rc-head .n')!.textContent, '2');
    equal(cards[0]!.querySelector('.verb')!.textContent, t('run.done'));
    ok(cards[0]!.querySelector('.time')!.textContent!.includes(t('time.hours', { n: 2 })), 'how long it worked');
    equal(cards[0]!.querySelector('.last')!.textContent, 'All the pages load again.');
    ok(!cards[0]!.classList.contains('need') && cards[1]!.classList.contains('need'), 'only an end that needs the person is amber');
    equal(cards[1]!.querySelector('.verb')!.textContent, t('run.needsYou'));
    cards[0]!.querySelector<HTMLButtonElement>('.name')!.click();
    cards[1]!.querySelector<HTMLButtonElement>('.dismiss')!.click();
    deepEqual([opened, dismissed], [['login'], ['build']]);
    setDone([]);
    await settle();
    equal(root.querySelector('.ri.done'), null);
    equal(root.querySelector('.rc-empty p')!.textContent, t('run.none'));
  } finally {
    dispose();
    root.remove();
  }
});
