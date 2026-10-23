import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Plan, WrapUp } from '../../ui/api.ts';
import { lang, t, useLanguage } from '../../ui/i18n.ts';
import { PlanList, WrapUpBox } from '../../ui/Plan.tsx';
import { isProgressCall } from '../../ui/worklog.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// Task 017: the agent's plan under the newest reply, and the closing box of a task.
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
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

check('closing box: preference and lesson Keep wait for the write, retry failures and remember kept proposals', async () => {
  const box: WrapUp = { recap: 'Done.', taste: 'Always use plain buttons.', lesson: 'Check paths before saving because folders can move.' };
  const [b, setB] = createSignal(box);
  const calls: string[] = [];
  let finish!: () => void;
  let reject!: () => void;
  const view = mount(() => createComponent(WrapUpBox, {
    get box() { return b(); }, open: false,
    onKeep: (kind) => {
      calls.push(kind);
      return new Promise<void>((resolve, fail) => { finish = resolve; reject = () => fail(new Error('write failed')); });
    },
  }));
  const buttons = () => [...view.root.querySelectorAll<HTMLButtonElement>('.wrap-note button')];
  try {
    await settle();
    equal(buttons().length, 2, 'older closing boxes still allow keeping a proposal');
    ok(buttons().every((button) => button.classList.contains('ghost')));
    buttons()[0].click(); await settle();
    deepEqual(calls, ['taste'], 'Keep needs no confirmation');
    ok(buttons().every((button) => button.disabled), 'a pending write cannot be submitted twice');
    reject(); await settle();
    equal(view.root.querySelector('[role="alert"]')!.textContent, t('error.memory.write'));
    equal(buttons()[0].disabled, false);
    buttons()[0].click(); finish(); await settle();
    equal(buttons().length, 1);
    equal(view.root.querySelector('.wrap-note [role="status"]')!.textContent, t('wrap.kept'));
    setB({ ...box, tasteKept: true, lessonKept: true }); await settle();
    equal(buttons().length, 0, 'saved flags show Kept after reopening');
    equal(view.root.querySelectorAll('.wrap-note [role="status"]').length, 2);
  } finally { view.done(); }
});
