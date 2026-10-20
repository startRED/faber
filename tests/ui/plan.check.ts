import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Plan, WrapUp } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
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
