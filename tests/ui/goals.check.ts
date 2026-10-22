import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type GoalRow, type GoalsView, type Project } from '../../ui/api.ts';
import { money, t, useLanguage } from '../../ui/i18n.ts';
import { ProjectGoals } from '../../ui/ProjectGoals.tsx';
import { ProjectsSection } from '../../ui/ProjectSettings.tsx';
import { check, equal, ok } from './check.ts';

// Standing goals (docs/design/projects.md, settings.md): a fold on the project page and one switch in Settings, Projects.
const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const deliberate = () => settle(560);
const row = (over: Partial<GoalRow> = {}): GoalRow => ({ id: 'g1', words: 'Every check passes', check: 'npm run lint', state: 'holding', lastRun: 1_800_000_000, ...over });
const view = (over: Partial<GoalsView> = {}): GoalsView => ({ auto: false, capSeen: false, capUsd: 2, spentUsd: 0.4, checks: ['npm run lint', 'cargo t'], canWatchCrashes: false, goals: [row()], ...over });
const button = (root: HTMLElement, key: Parameters<typeof t>[0]) => {
  const found = [...root.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent === t(key));
  ok(found, `missing button: ${key}`);
  return found;
};

async function withFold(mock: () => void, run: (root: HTMLElement) => Promise<void>, width = '640px') {
  const originals = { ...api };
  mock();
  useLanguage('en');
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  root.style.width = width;
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(ProjectGoals, { projectId: 'goal-project' }), root);
  try { await run(root); }
  finally { dispose(); root.remove(); Object.assign(api, originals); useLanguage('en'); }
}

check('project goals: the fold reads only when opened, shows each goal with its state and check, and works with no AI connected', () => {
  let reads = 0;
  return withFold(() => {
    api.projectGoals = async (id) => { equal(id, 'goal-project'); reads++; return view({ goals: [row(), row({ id: 'g2', words: 'No crashes', state: 'needsYou', reason: 'reason.goal.fixTried', lastRun: undefined })] }); };
  }, async (root) => {
    equal(reads, 0, 'a closed fold reads nothing');
    button(root, 'goals.title').click(); await settle();
    equal(reads, 1);
    const text = root.textContent!;
    ok(text.includes('Every check passes') && text.includes(t('goals.state.holding')));
    ok(text.includes(t('goals.state.needsYou')) && text.includes(t('goals.never')) && text.includes(t('reason.goal.fixTried')));
    ok(text.includes(t('goals.off')), 'with the switch off the fold says so');
    equal(root.querySelectorAll('.goal code')[0].textContent, 'npm run lint');
    equal(root.querySelectorAll('.goal')[0].querySelector('[class~="btn"]:not(.danger)'), null, 'a holding goal offers only Remove');
    ok(button(root, 'goals.resume'), 'a goal that needs the person can be watched again');
  });
});

check('project goals: add takes words and one confirmed check; with no confirmed check there is a hint instead of a form', async () => {
  let added: string[] = [];
  let list = view({ goals: [], checks: [] });
  await withFold(() => {
    api.projectGoals = async () => list;
    api.projectGoalAdd = async (_id, words, command) => { added = [words, command]; return view({ goals: [row({ words, check: command })] }); };
  }, async (root) => {
    button(root, 'goals.title').click(); await settle();
    ok(root.textContent!.includes(t('goals.empty')) && root.textContent!.includes(t('goals.noChecks')));
    equal(root.querySelector('form'), null, 'no form without a confirmed check');
    list = view({ goals: [] });
    button(root, 'goals.refresh').click(); await settle();
    const words = root.querySelector<HTMLInputElement>('.goal-add input')!;
    words.value = 'It builds'; words.dispatchEvent(new Event('input', { bubbles: true }));
    const pick = root.querySelector<HTMLSelectElement>('.goal-add select')!;
    pick.value = 'cargo t'; pick.dispatchEvent(new Event('change', { bubbles: true }));
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
    equal(added.join('|'), 'It builds|cargo t');
    ok(root.textContent!.includes('It builds'));
    equal(root.querySelector<HTMLInputElement>('.goal-add input')!.value, '', 'the words are cleared once it is added');
  });
});

check('project goals: only Faber\'s own project offers a crash goal, beside the check one, with the same words and no check to pick', async () => {
  let added: string[] = [];
  let own = false;
  await withFold(() => {
    api.projectGoals = async () => view({ goals: [], checks: [], canWatchCrashes: own });
    api.projectGoalAddCrashes = async (_id, words) => { added = [words]; return view({ goals: [row({ words, check: '', crashes: true })], checks: [], canWatchCrashes: true }); };
  }, async (root) => {
    button(root, 'goals.title').click(); await settle();
    equal([...root.querySelectorAll('button')].some((b) => b.textContent === t('goals.addCrashes')), false, 'another project is not offered it');
    own = true;
    button(root, 'goals.refresh').click(); await settle();
    ok(root.textContent!.includes(t('goals.noChecks')), 'with no confirmed check the check goal still says how to get one');
    equal(root.querySelector('.goal-add select'), null, 'a crash goal has no check to pick');
    const watch = button(root, 'goals.addCrashes');
    ok(watch.disabled, 'it needs the words first, as the other add does');
    const words = root.querySelector<HTMLInputElement>('.goal-add input')!;
    words.value = 'No new crashes'; words.dispatchEvent(new Event('input', { bubbles: true }));
    equal(watch.disabled, false);
    watch.click(); await settle();
    equal(added.join('|'), 'No new crashes');
    const text = root.querySelector('.goal')!.textContent!;
    ok(text.includes('No new crashes') && text.includes(t('goals.crashSource')), 'the row names the record it watches');
    equal(root.querySelector('.goal code'), null, 'and shows no check');
    equal(root.querySelector<HTMLInputElement>('.goal-add input')!.value, '', 'the words are cleared once it is added');
  });
});

check('project goals: remove asks first, a double click cannot confirm, resume goes to the engine', async () => {
  let removed = 0;
  let resumed = 0;
  await withFold(() => {
    api.projectGoals = async () => view({ goals: [row({ state: 'paused' })] });
    api.projectGoalRemove = async (_id, goal) => { equal(goal, 'g1'); removed++; return view({ goals: [] }); };
    api.projectGoalResume = async (_id, goal) => { equal(goal, 'g1'); resumed++; return view({ goals: [row()] }); };
  }, async (root) => {
    button(root, 'goals.title').click(); await settle();
    button(root, 'goals.resume').click(); await settle();
    equal(resumed, 1);
    button(root, 'goals.remove').click(); await settle(30);
    button(root, 'goals.confirmRemove').click(); await settle();
    equal(removed, 0, 'two quick activations only arm it');
    await deliberate();
    button(root, 'goals.confirmRemove').click(); await settle();
    equal(removed, 1);
    ok(root.textContent!.includes(t('goals.empty')));
  });
});

check('project goals: German at 440 px does not overflow, and a refused add shows its reason', async () => {
  await withFold(() => {
    api.projectGoals = async () => view({ goals: [row({ state: 'needsYou', reason: 'reason.goal.billing', words: 'Die Anwendung startet in unter zwei Sekunden und stürzt nie ab' })] });
    api.projectGoalAdd = async () => { throw 'reason.goal.many'; };
  }, async (root) => {
    useLanguage('de');
    button(root, 'goals.title').click(); await settle();
    await document.fonts.ready;
    ok(root.scrollWidth <= root.clientWidth + 1, 'no horizontal overflow in a narrow column');
    const words = root.querySelector<HTMLInputElement>('.goal-add input')!;
    words.value = 'x'; words.dispatchEvent(new Event('input', { bubbles: true }));
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
    ok(root.textContent!.includes(t('reason.goal.many')));
    equal(root.querySelector<HTMLInputElement>('.goal-add input')!.value, 'x', 'a refused add keeps the words');
  }, '440px');
});

check('settings projects: one switch per project shows the day limit and what was spent, and turning it on is sent to the engine', async () => {
  const originals = { ...api };
  let on = false;
  api.projectProof = async () => ({ commands: [], declared: [], source: 'none' as const, confirmed: false, changed: false, setup: [] });
  api.projectGoals = async () => view({ auto: on });
  api.projectGoalsAuto = async (_id, next) => { on = next; return view({ auto: next, capSeen: true }); };
  useLanguage('en');
  const project: Project = { id: 'goal-project', name: 'Goal project', path: 'C:/fixture/goals' };
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:700px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(ProjectsSection, { projects: [project] }), root);
  try {
    await settle();
    root.querySelector<HTMLElement>('.pset-head')!.click(); await settle();
    const sw = root.querySelector<HTMLInputElement>('#pset-goal-project-goals-switch')!;
    ok(sw, 'the switch is a row of the project card');
    equal(sw.checked, false, 'off until the person turns it on');
    ok(root.textContent!.includes(t('set.projects.goalsHint', { cap: money(2, 0), spent: money(0.4) })), 'the limit and today\'s spending are always in view');
    sw.click(); await settle();
    equal(on, true);
    equal(root.querySelector<HTMLInputElement>('#pset-goal-project-goals-switch')!.checked, true);
  } finally { dispose(); root.remove(); Object.assign(api, originals); }
});

check('project goals: a first read that fails shows its reason, and with no goal and the switch off there is one hint, not two', async () => {
  let failing = true;
  await withFold(() => {
    api.projectGoals = async () => { if (failing) throw 'error.store.corrupt'; return view({ goals: [] }); };
  }, async (root) => {
    button(root, 'goals.title').click(); await settle();
    equal(root.querySelector('.note.err')?.textContent, t('error.store.corrupt'), 'the reason shows although nothing was read');
    failing = false;
    button(root, 'goals.refresh').click(); await settle();
    equal(root.querySelector('.note.err'), null, 'a good read clears it');
    ok(root.textContent!.includes(t('goals.empty')) && !root.textContent!.includes(t('goals.off')), 'no goal: only the empty line');
  });
});

check('settings projects: a goals read that fails says why in the switch row, and the switch stays off', async () => {
  const originals = { ...api };
  api.projectProof = async () => ({ commands: [], declared: [], source: 'none' as const, confirmed: false, changed: false, setup: [] });
  api.projectGoals = async () => { throw 'error.store.corrupt'; };
  useLanguage('en');
  const project: Project = { id: 'goal-project', name: 'Goal project', path: 'C:/fixture/goals' };
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:700px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(ProjectsSection, { projects: [project] }), root);
  try {
    await settle();
    root.querySelector<HTMLElement>('.pset-head')!.click(); await settle();
    const sw = root.querySelector<HTMLInputElement>('#pset-goal-project-goals-switch')!;
    ok(sw.disabled && !sw.checked, 'nothing to switch while the goals cannot be read');
    ok(root.textContent!.includes(t('error.store.corrupt')), 'the row says why');
  } finally { dispose(); root.remove(); Object.assign(api, originals); }
});

check('project goals: words typed for a new goal survive closing the fold and building the page again', async () => {
  const mock = () => { api.projectGoals = async () => view(); };
  await withFold(mock, async (root) => {
    button(root, 'goals.title').click(); await settle();
    const words = root.querySelector<HTMLInputElement>('.goal-add input')!;
    words.value = 'It builds'; words.dispatchEvent(new Event('input', { bubbles: true }));
  });
  try {
    await withFold(mock, async (root) => {
      button(root, 'goals.title').click(); await settle();
      equal(root.querySelector<HTMLInputElement>('.goal-add input')!.value, 'It builds');
    });
  } finally { localStorage.removeItem('faber.unsent.goal.goal-project'); }
});
