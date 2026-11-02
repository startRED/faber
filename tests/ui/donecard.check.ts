import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Summary } from '../../ui/api.ts';
import { lang, LANGUAGES, t, useLanguage } from '../../ui/i18n.ts';
import { Running } from '../../ui/Running.tsx';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

// Task 016 (the legible agent runs design): a run that ends while nobody looks keeps its place in the running column as a calm
// card; a click opens the conversation, its × only clears it.
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
const card = (id: string, kind: NonNullable<Summary['ended']>['kind'], over: Partial<Summary> = {}): Summary => ({
  id,
  title: `Fix ${id}`,
  area: 'project',
  projectId: 'p1',
  updatedAt: 0,
  ended: { at: 1, kind, workMs: 2 * 3_600_000 + 600_000, line: 'All the pages load again.' },
  ...over,
});

check('done card: a run that ended shows how it ended, opens with a click and goes with its ×', async () => {
  const before = lang();
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
    const pending = 'Verify the saved work; Write the final report';
    setDone([card('conclusion', 'unconcluded', { ended: { at: 1, kind: 'unconcluded', workMs: 1, line: pending } })]);
    for (const { code } of LANGUAGES) {
      useLanguage(code);
      await settle();
      const unfinished = root.querySelector<HTMLElement>('.ri.done')!;
      equal(unfinished.querySelector('.verb')!.textContent, t('run.unconcluded'), code);
      ok(unfinished.querySelector('.verb')!.textContent !== t('run.done'), code + ' cannot claim done');
      ok(unfinished.classList.contains('need'), code + ' keeps unfinished work visible');
      equal(unfinished.querySelector('.last')!.textContent, pending);
      equal(unfinished.querySelector('.last')!.getAttribute('title'), pending, 'full pending step names remain accessible');
    }
    setDone([]);
    await settle();
    equal(root.querySelector('.ri.done'), null);
    equal(root.querySelector('.rc-empty p')!.textContent, t('run.none'));
  } finally {
    useLanguage(before);
    dispose();
    root.remove();
  }
});

check('done card: a run cut by Faber closing says so and is never "done"', async () => {
  const [done, setDone] = createSignal<Summary[]>([card('cut', 'interrupted', { ended: { at: 1, kind: 'interrupted', workMs: 1, line: 'Wrote the migration.' } })]);
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Running, {
    items: [],
    get done() { return done(); },
    projects: [],
    onOpen: () => {},
    onOpenDone: () => {},
    onDismiss: () => {},
    onClose: () => {},
    modal: false,
    ref: () => {},
  }), root);
  try {
    await settle();
    const verb = root.querySelector('.ri.done .verb')!.textContent;
    equal(verb, t('run.interrupted'));
    ok(verb !== t('run.done'), 'cut work cannot claim done');
    ok(root.querySelector('.ri.done')!.classList.contains('need'), 'it stays visible like unfinished work');
    setDone([]);
  } finally {
    dispose();
    root.remove();
  }
});

check('sidebar: a conversation the AI never concluded, cut by a close, stuck or failed says so; a concluded one says nothing', async () => {
  const now = Date.now();
  const row = (id: string, state: NonNullable<Summary['state']>): Summary => ({ id, title: `Task ${id}`, area: 'project', projectId: 'p', updatedAt: now, state });
  const states = ['concluded', 'unconcluded', 'interrupted', 'stuck', 'failed', 'stopped', 'waiting'] as const;
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:360px;height:520px';
  root.dataset.motion = 'none';
  document.body.append(root);
  const dispose = render(() => createComponent(Sidebar, {
    conversations: states.map((s) => row(s, s)),
    projects: [{ id: 'p', name: 'Garden', path: 'X:/garden' }], vendors: [], version: 'test',
    place: { area: 'project', projectId: 'p' }, running: [], quotas: {}, now,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
    onOpen: () => {}, onRemove: () => {}, onSettings: () => {}, onError: () => {},
  }), root);
  try {
    await settle();
    const mark = (id: string) => [...root.querySelectorAll('.crow')].find((r) => r.textContent!.includes(`Task ${id}`))?.querySelector('.state-mark')?.textContent ?? null;
    equal(mark('concluded'), null, 'concluded: nothing to add');
    equal(mark('stopped'), null, 'the person knows they stopped it');
    equal(mark('waiting'), null, 'waiting has its place in the running list');
    equal(mark('unconcluded'), t('run.unconcluded'));
    equal(mark('interrupted'), t('run.interrupted'));
    equal(mark('stuck'), t('run.stuckNoResult'));
    equal(mark('failed'), t('run.doneFailed'));
  } finally {
    dispose();
    root.remove();
  }
});
