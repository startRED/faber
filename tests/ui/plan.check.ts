import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Message, Plan, WrapUp } from '../../ui/api.ts';
import { lang, t, useLanguage } from '../../ui/i18n.ts';
import { PlanList, WrapUpBox } from '../../ui/Plan.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { isProgressCall } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// Task 017: the agent's plan under the newest reply, and the closing box of a task.
const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const mount = (view: () => unknown) => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(view as () => Element, root);
  return { root, done: () => (dispose(), root.remove()) };
};

check('plan: done steps are checked, the current one is marked, and the count follows the plan', async () => {
  const plan = (done: number): Plan => ({
    at: 1,
    steps: ['Read the page', 'Fix the redirect', 'Check it works'].map((text, i) => ({
      text,
      state: i < done ? 'done' : i === done ? 'active' : 'pending',
    })),
  });
  const [p, setP] = createSignal(plan(1));
  const view = mount(() => createComponent(PlanList, { get plan() { return p(); } }));
  try {
    await settle();
    const steps = [...view.root.querySelectorAll('.plan li')];
    deepEqual(steps.map((s) => s.className), ['done', 'active', 'pending']);
    equal(steps[1]!.getAttribute('aria-current'), 'step');
    ok(steps[0]!.querySelector('.mark svg'), 'a done step shows its check');
    equal(steps[2]!.querySelector('.sr')!.textContent, t('plan.state.pending'), 'a screen reader hears each state');
    equal(view.root.querySelector('.plan-head .num')!.textContent, t('plan.count', { done: 1, total: 3 }));
    setP(plan(2));
    await settle();
    equal(view.root.querySelector('.plan-head .num')!.textContent, t('plan.count', { done: 2, total: 3 }));
  } finally {
    view.done();
  }
});

check('plan: a done step shows its evidence as its tooltip and nothing else', async () => {
  const plan: Plan = {
    at: 1,
    steps: [
      { text: 'Read the page', state: 'done', evidence: 'Read ui/Plan.tsx lines 1-42.' },
      { text: 'Fix the redirect', state: 'active', evidence: 'Ignored while active' },
      { text: 'Check it works', state: 'pending' },
    ],
  };
  const view = mount(() => createComponent(PlanList, { plan }));
  try {
    await settle();
    const steps = [...view.root.querySelectorAll<HTMLLIElement>('.plan li')];
    equal(steps[0]!.getAttribute('title'), 'Read ui/Plan.tsx lines 1-42.');
    equal(steps[0]!.querySelector('.text')!.textContent, 'Read the page');
    equal(steps[1]!.getAttribute('title'), null);
    equal(steps[2]!.getAttribute('title'), null);
  } finally {
    view.done();
  }
});

check('plan: the economy line shows only once the estimate is passed, and fits 720 px in the longest language', async () => {
  const plan = (economy?: boolean): Plan => ({
    at: 1,
    steps: [{ text: 'Build it', state: 'active' }],
    ...(economy === undefined ? {} : { estimate: { cents: 100, base: 0, economy } }),
  });
  const before = lang();
  const [p, setP] = createSignal(plan(false));
  const view = mount(() => createComponent(PlanList, { get plan() { return p(); } }));
  view.root.style.cssText = 'position:fixed;left:0;top:0;width:720px';
  try {
    await settle();
    equal(view.root.querySelector('.plan .note'), null, 'an estimate not passed says nothing');
    setP(plan());
    await settle();
    equal(view.root.querySelector('.plan .note'), null, 'no estimate, no line');
    setP(plan(true));
    for (const code of ['en', 'pt-BR', 'es', 'fr', 'de']) {
      useLanguage(code);
      await settle();
      const note = view.root.querySelector<HTMLElement>('.plan .note')!;
      equal(note.textContent, t('plan.economy'), code);
      ok(!/\d/.test(note.textContent!), 'no numbers in the line');
      ok(note.getBoundingClientRect().right <= view.root.getBoundingClientRect().right + 1, `${code} stays inside 720 px`);
      ok(note.scrollWidth <= note.clientWidth + 1, `${code} does not overflow`);
    }
  } finally {
    useLanguage(before);
    view.done();
  }
});

check('closing box: the recap, one question, and buttons that reply for the person, the recommendation first', async () => {
  const sent: string[] = [];
  const box: WrapUp = { recap: 'The login works. Not seen on a phone yet.', ask: 'Check it on a phone?', actions: ['Yes', 'Later'] };
  const [b, setB] = createSignal(box);
  const [open, setOpen] = createSignal(true);
  const view = mount(() =>
    createComponent(WrapUpBox, { get box() { return b(); }, get open() { return open(); }, onReply: (text: string) => sent.push(text) }),
  );
  try {
    await settle();
    equal(view.root.querySelector('.wrap-up .recap')!.textContent, box.recap);
    equal(view.root.querySelector('.wrap-ask .q')!.textContent, box.ask);
    const buttons = [...view.root.querySelectorAll<HTMLButtonElement>('.wrap-actions button')];
    deepEqual(buttons.map((x) => [x.textContent, x.classList.contains('primary')]), [['Yes', true], ['Later', false]]);
    buttons[1]!.click();
    deepEqual(sent, ['Later']);
    // An older reply's box keeps its words but offers no buttons.
    setOpen(false);
    await settle();
    equal(view.root.querySelector('.wrap-actions'), null);
    // A question Faber answered while the person was away offers no buttons: the reply's answers card says what it chose.
    setOpen(true);
    setB({ ...box, answered: 'Yes' });
    await settle();
    equal(view.root.querySelector('.wrap-actions'), null);
    equal(view.root.querySelector('.wrap-ask .q')!.textContent, box.ask);
  } finally {
    view.done();
  }
});

check('plan: the calls that keep the plan, the now line and the box are never steps of the work log', () => {
  const call = (name: string) => ({ id: name, name, input: {} });
  deepEqual(['Plan', 'Status', 'WrapUp', 'Bash', 'Agent'].map((n) => isProgressCall(call(n))), [true, true, true, false, false]);
});

check('plan: a working reply shows the current thinking headline after its words and passes the active step detail to the plan', async () => {
  const messages: Message[] = [
    { id: 'u1', role: 'user', text: 'Ship the fix', at: 1 },
    {
      id: 'a1',
      role: 'assistant',
      text: 'Checking the test suite now.',
      at: 2,
      status: 'streaming',
      thinking: '**Auditing failing assertions**',
      tools: [{ id: 't1', name: 'Bash', input: { command: 'npm test' }, ok: true }],
    },
  ];
  const plan: Plan = {
    at: 2,
    steps: [
      { text: 'Read the logs', state: 'done' },
      { text: 'Run the test suite', state: 'active' },
    ],
  };
  const view = mount(() => createComponent(Thread, { conversationId: 'c1', messages, from: 0, options: [], onBranch: () => {}, plan }));
  try {
    await settle();
    equal(view.root.querySelector('.plan li.active .now')!.textContent, ' · Auditing failing assertions');
    await settle(1_600);
    view.root.querySelector<HTMLButtonElement>('.steps-head')!.click();
    ok(view.root.querySelector('.thought .step-line')!.textContent!.includes('Auditing failing assertions'));
  } finally {
    view.done();
  }
});

check('closing box: a conclusion with its summary is the AI\'s own last message, so the box keeps only its question', async () => {
  const summary = { did: 'Fixed the redirect.', result: 'Login works.', verified: 'Ran the login test.' };
  const done: WrapUp = { recap: 'Login works.', done: true, outcome: 'done', summary };
  const [b, setB] = createSignal<WrapUp>(done);
  const [written, setWritten] = createSignal(true);
  const view = mount(() => createComponent(WrapUpBox, { get box() { return b(); }, get written() { return written(); }, open: true, onReply: () => {} }));
  try {
    await settle();
    equal(view.root.querySelector('.wrap-up'), null, 'a done conclusion draws nothing beside the AI\'s own message');
    setWritten(false);
    await settle();
    equal(view.root.querySelector('.wrap-up .recap')!.textContent, 'Login works.', 'with no closing message of its own, the recap stays');
    setWritten(true);
    await settle();
    setB({ recap: 'Login works. Not seen on a phone.', outcome: 'needsYou', summary, ask: 'Check it on a phone?', actions: ['Yes', 'Later'] });
    await settle();
    equal(view.root.querySelector('.wrap-up .recap'), null, 'the recap is not repeated');
    equal(view.root.querySelector('.wrap-ask .q')!.textContent, 'Check it on a phone?');
  } finally {
    view.done();
  }
});

check('plan: a step another conversation does names it with where it stands, and a click opens it, in German at 720 px', async () => {
  useLanguage('de');
  const opened: string[] = [];
  const plan: Plan = {
    at: 1,
    steps: [
      { text: 'Die Dokumentation schreiben', state: 'active', conversation: 'c-docs', member: 'waiting' },
      { text: 'Den Parser prüfen', state: 'pending', conversation: 'c-parser', member: 'running' },
      { text: 'Alles zusammenführen', state: 'pending' },
    ],
  };
  const view = mount(() =>
    createComponent(PlanList, { plan, names: { 'c-docs': 'Dokumentation', 'c-parser': 'Parser prüfen' }, onOpen: (id: string) => opened.push(id) }),
  );
  view.root.style.width = '720px';
  try {
    await settle();
    const steps = [...view.root.querySelectorAll<HTMLElement>('.plan li')];
    const links = steps.map((s) => s.querySelector<HTMLButtonElement>('.member button'));
    deepEqual(links.map((b) => b?.textContent ?? null), ['Dokumentation', 'Parser prüfen', null], 'the title of the conversation doing each step');
    deepEqual(steps.map((s) => s.querySelector('.member-state')?.textContent ?? null), [` · ${t('agent.waiting')}`, ` · ${t('agent.running')}`, null], 'where it stands, in the words of a helper row');
    links[1]!.click();
    deepEqual(opened, ['c-parser'], 'a click opens that conversation');
    const text = steps[0]!.querySelector<HTMLElement>('.text')!.getBoundingClientRect();
    const member = steps[0]!.querySelector<HTMLElement>('.member')!.getBoundingClientRect();
    ok(member.top >= text.bottom - 1 && member.right <= view.root.getBoundingClientRect().right + 0.5, 'under the step, inside the column');
  } finally {
    useLanguage('en');
    view.done();
  }
});
