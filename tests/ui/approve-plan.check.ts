import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { PermissionChip } from '../../ui/Composer.tsx';
import { toolRow } from '../../ui/worklog.ts';
import { PlanCard } from '../../ui/Ask.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { api, type PermissionMode } from '../../ui/api.ts';
import { LANGUAGES, t, useLanguage } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

check('approve-plan: a closing box keeps its recap without a second approval action', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Thread, {
    conversationId: 'conversation', options: [], from: 0, onBranch: () => {}, onReply: () => {},
    messages: [{ id: 'turn', role: 'assistant', text: 'The proposed plan.', at: 1, vendor: 'codex', status: 'done',
      permissions: [{ requestId: 'plan:item', tool: 'ExitPlanMode', input: {} }],
      wrapUp: { recap: 'The plan is ready.', ask: 'Start?', actions: ['Start', 'Later'] } }],
  }), root);
  try {
    ok(root.querySelector('.wrap-up .recap'));
    equal(root.querySelector('.wrap-actions'), null);
    equal(root.querySelectorAll('.plan-approval .primary').length, 1);
  } finally { dispose(); root.remove(); }
});

check('approve-plan: the card chooses a run mode, excludes Plan, and keeps planning without a mode', async () => {
  const original = api.approvePlan;
  const calls: [string, string, string, PermissionMode | undefined][] = [];
  api.approvePlan = async (id, turn, request, mode) => { calls.push([id, turn, request, mode]); };
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(PlanCard, {
    conversationId: 'conversation', turnId: 'turn', permission: { requestId: 'request', tool: 'ExitPlanMode', input: { mode: 'ask' } },
  }), root);
  try {
    const select = root.querySelector<HTMLButtonElement>('[role=combobox]')!;
    equal(select.textContent, t('set.permissions.ask'), 'the previous mode is offered');
    select.click();
    deepEqual([...root.querySelectorAll('[role=option] b')].map((o) => o.textContent), (['ask', 'acceptEdits', 'auto', 'dontAsk'] as const).map((m) => t(`set.permissions.${m}`)));
    const [approve, keep] = [...root.querySelectorAll<HTMLButtonElement>('.plan-approval .btn')];
    equal(approve.textContent, t('ask.plan.approve'));
    ok(approve.classList.contains('primary'));
    equal(keep.textContent, t('ask.plan.keep'));
    (root.querySelectorAll('[role=option]')[2] as HTMLElement).click();
    approve.click();
    await Promise.resolve();
    deepEqual(calls, [['conversation', 'turn', 'request', 'auto']]);
    keep.click();
    await Promise.resolve();
    deepEqual(calls[1], ['conversation', 'turn', 'request', undefined]);
  } finally {
    dispose(); root.remove(); api.approvePlan = original;
  }
});

check('approve-plan: unknown plan history starts with Ask and an error keeps that choice', async () => {
  const original = api.approvePlan;
  api.approvePlan = async () => { throw new Error('error.permission.undelivered'); };
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(PlanCard, {
    conversationId: 'conversation', turnId: 'turn', permission: { requestId: 'request', tool: 'ExitPlanMode', input: {} },
  }), root);
  try {
    const select = root.querySelector<HTMLButtonElement>('[role=combobox]')!;
    equal(select.textContent, t('set.permissions.ask'));
    root.querySelector<HTMLButtonElement>('.plan-approval .primary')!.click();
    await Promise.resolve(); await Promise.resolve();
    equal(select.disabled, false);
    equal(select.textContent, t('set.permissions.ask'));
    ok(root.querySelector('[role=alert]'), 'the card keeps the delivery failure');
  } finally {
    dispose(); root.remove(); api.approvePlan = original;
  }
});

check('approve-plan: a tool-only plan is safe Markdown in the card and detail in the work log', () => {
  const plan = '1. Inspect the **parser**.\n2. Add the check.\n\n<span onclick="alert(1)">Safe text</span>';
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(PlanCard, {
    conversationId: 'conversation', turnId: 'turn', permission: { requestId: 'request', tool: 'ExitPlanMode', input: { plan } },
  }), root);
  try {
    equal(root.querySelector('.md ol li strong')?.textContent, 'parser');
    equal(root.querySelectorAll('.md li').length, 2);
    equal(getComputedStyle(root.querySelector('.plan-approval .md')!).display, 'block', 'the plan keeps the reply’s block layout');
    equal(root.querySelector('.md [onclick]'), null, 'the same Markdown sanitizer as replies protects tool input');
    equal(toolRow({ id: 'request', name: 'ExitPlanMode', input: { plan } }).detail, plan);
  } finally { dispose(); root.remove(); }
});

check('approve-plan: mode choices share the composer chip words and one-line meanings', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => [createComponent(PermissionChip, { mode: 'ask', onMode: () => {} }), createComponent(PlanCard, { conversationId: 'conversation', turnId: 'turn', permission: { requestId: 'request', tool: 'ExitPlanMode', input: { mode: 'ask' } } })], root);
  try {
    const chips = root.querySelectorAll<HTMLButtonElement>('[role=combobox]');
    chips[0]!.click(); chips[1]!.click();
    const options = (selector: string) => [...root.querySelectorAll(selector)].map(o => [o.querySelector('b')?.textContent, o.querySelector('small')?.textContent]).filter(o => o[0] !== t('set.permissions.plan'));
    deepEqual(options('.plan-approval [role=option]'), options(':scope > .permission-chip [role=option]'));
    ok(options('.plan-approval [role=option]').every(o => o[1]), 'every mode has its one-line meaning');
    equal(root.querySelector('.plan-approval .permission-chip')?.getAttribute('title'), t('set.permissions.ask.line'));
  } finally { dispose(); root.remove(); }
});

check('approve-plan: with no editable focus the thread focuses approval and announces it once', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  equal(document.activeElement, document.body, 'no field is being edited');
  const permission = { requestId: 'request', tool: 'ExitPlanMode', input: {} };
  const dispose = render(() => [createComponent(PlanCard, { conversationId: 'conversation', turnId: 'turn', permission, surface: 'ask' }), createComponent(PlanCard, { conversationId: 'conversation', turnId: 'turn', permission })], root);
  try {
    await Promise.resolve();
    equal(document.activeElement, root.querySelector('[data-surface=ask] .primary'));
    equal(root.querySelectorAll('[role=status][aria-live=polite]').length, 1, 'the thread announces once; its column copy stays quiet');
  } finally { dispose(); root.remove(); }
});

check('approve-plan: a plan card leaves focus on the person typing in any editable field', async () => {
  for (const tag of ['textarea', 'input', 'div']) {
    const root = document.createElement('div');
    const field = document.createElement(tag);
    if (tag === 'div') { field.contentEditable = 'true'; field.tabIndex = 0; }
    document.body.append(field, root);
    field.focus();
    const dispose = render(() => createComponent(PlanCard, {
      conversationId: 'conversation', turnId: 'turn', surface: 'ask', permission: { requestId: 'request', tool: 'ExitPlanMode', input: {} },
    }), root);
    try {
      await Promise.resolve();
      equal(document.activeElement, field, tag + ' keeps typing focus');
      ok(root.querySelector('[role=status][aria-live=polite]'), 'the plan is announced without interrupting typing');
    } finally { dispose(); root.remove(); field.remove(); }
  }
});

check('approve-plan: every language explains the shared mode and quietly names Faber starting the plan', () => {
  const lines = {
    en: ['This choice sets the permission mode for all conversations.', 'Faber started the approved plan'],
    'pt-BR': ['Essa escolha define as permissões de todas as conversas.', 'O Faber iniciou o plano aprovado'],
    es: ['Esta elección cambia los permisos de todas las conversaciones.', 'Faber inició el plan aprobado'],
    fr: ['Ce choix définit les autorisations de toutes les conversations.', 'Faber a lancé le plan approuvé'],
    de: ['Diese Auswahl legt die Berechtigungen für alle Unterhaltungen fest.', 'Faber hat den genehmigten Plan gestartet'],
  };
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => [
    createComponent(PlanCard, { conversationId: 'conversation', turnId: 'turn', permission: { requestId: 'request', tool: 'ExitPlanMode', input: {} } }),
    createComponent(Thread, { conversationId: 'conversation', options: [], from: 0, onBranch: () => {}, messages: [
      { id: 'prompt', role: 'user', text: 'Go ahead with the plan.', at: 0, wake: { why: 'wake.approvedPlan' } },
      { id: 'reply', role: 'assistant', text: 'Started the approved work.', at: 1, status: 'done' },
    ] }),
  ], root);
  try {
    for (const language of LANGUAGES) {
      useLanguage(language.code);
      equal(root.querySelector('.plan-approval .note')?.textContent, lines[language.code][0]);
      equal(root.querySelector('.by-faber')?.getAttribute('aria-label'), lines[language.code][1]);
      equal(language.messages['ask.plan.shared'], lines[language.code][0]);
      equal(language.messages['wake.approvedPlan'], lines[language.code][1]);
      ok(!root.textContent?.includes('Go ahead with the plan.'), 'the engine instruction takes no user message row');
    }
  } finally { dispose(); root.remove(); useLanguage('en'); }
});

check('approve-plan: a later permission choice refuses approval on the card and leaves Keep planning available', async () => {
  const original = api.approvePlan;
  const calls: (PermissionMode | undefined)[] = [];
  api.approvePlan = async (_id, _turn, _request, mode) => {
    calls.push(mode);
    if (mode) throw new Error('error.permission.planChanged');
  };
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(PlanCard, {
    conversationId: 'conversation', turnId: 'turn', permission: { requestId: 'request', tool: 'ExitPlanMode', input: { mode: 'dontAsk' } },
  }), root);
  try {
    const [approve, keep] = [...root.querySelectorAll<HTMLButtonElement>('.plan-approval .btn')];
    approve.click();
    await Promise.resolve(); await Promise.resolve();
    equal(root.querySelector('[role=alert]')?.textContent, t('error.permission.planChanged'));
    equal(approve.disabled, true, 'the outdated card cannot submit another approval');
    equal(root.querySelector<HTMLButtonElement>('[role=combobox]')?.disabled, true);
    equal(keep.disabled, false, 'Keep planning does not change permissions');
    keep.click();
    await Promise.resolve();
    deepEqual(calls, ['dontAsk', undefined]);
  } finally { dispose(); root.remove(); api.approvePlan = original; }
});
