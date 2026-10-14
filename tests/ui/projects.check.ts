import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type AppState, type MissionView, type Project } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { LANGUAGES, useLanguage } from '../../ui/i18n.ts';
import { Projects } from '../../ui/Projects.tsx';
import { formatCap, formatCost, recallGoal, saveGoal, taskReceiptStatus } from '../../ui/projects.ts';
import { check, equal, ok } from './check.ts';

const demo: Project = { id: 'review-project', name: 'Review project', path: 'C:/fixture/review' };
const view = (revision = 1): MissionView => ({ missionId: 'review-mission', revision, state: 'running', mode: 'normal', gitMode: 'seguro', goal: 'Review goal', tasks: [], cost: { ownMicros: null, capMicros: null, complete: false }, pace: [], decisions: [] });

async function withProjects(run: (root: HTMLElement, emit: (v: MissionView) => void, change: (p: Project) => void) => Promise<void>) {
  const originals = { ...api };
  useLanguage('en');
  Object.assign(api, { localUpdateState: async () => null });
  let listener: (v: MissionView) => void = () => {};
  api.projectEnroll = async (id) => ({ projectId: id, repo: demo.path, base: 'base', mode: 'seguro', revision: 1, selfUpdate: false });
  api.projectGrant = async () => ({ repo: demo.path, mode: 'seguro', revision: 1, grantedAt: 1, selfUpdate: false });
  api.missionStart = async () => 'review-mission';
  api.missionState = async () => view();
  api.onMission = async (fn) => { listener = fn; return () => {}; };
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const [project, setProject] = createSignal(demo);
  const dispose = render(() => createComponent(Projects, { get project() { return project(); }, vendors: [], now: Date.now() }), root);
  try { await settle(); await run(root, (v) => listener(v), setProject); }
  finally { dispose(); root.remove(); Object.assign(api, originals); saveGoal(demo.id, ''); localStorage.removeItem('faber.project.mission.' + demo.id); useLanguage('en'); }
}

check('projects: local build status uses the read-only channel view before a mission exists', () => withProjects(async (root) => {
  Object.assign(api, { localUpdateState: async () => ({ current: 'slot-b', next: 'slot-a', lastGood: 'slot-b', waiting: 'projects.task.review' }) });
  // A project identity refresh re-reads the development view without creating work.
  window.dispatchEvent(new Event('focus'));
  await settle();
  const card = root.querySelector('[data-testid="local-build"]');
  ok(card, 'development status must be available without starting a mission');
  ok(card!.textContent?.includes('slot-b'), 'current slot must come from local_update_state');
  ok(card!.textContent?.includes('slot-a'), 'next slot must come from local_update_state');
}));

check('projects: build card displays live slot status and explains why promotion waits', () => withProjects(async (root, emit) => {
  useLanguage('en');
  let callCount = 0;
  let status = {
    current: 'A · 111122223333',
    next: 'B · 444455556666',
    lastGood: 'A · 111122223333',
    waiting: 'reason.update.quiescence',
  };
  api.localUpdateState = async () => {
    callCount++;
    return status;
  };
  window.dispatchEvent(new Event('focus'));
  await settle();

  const card = root.querySelector('[data-testid="local-build"]');
  ok(card, 'build card must be rendered');
  ok(card!.textContent?.includes('A · 111122223333'), 'current slot is displayed');
  ok(card!.textContent?.includes('B · 444455556666'), 'next slot is displayed');
  ok(card!.textContent?.includes('Waiting for:'), 'waiting label is displayed');
  ok(card!.textContent?.includes('Promotion waits for active operations'), 'quiescence waiting reason is displayed');

  // Verify hand-written pt-BR localization
  useLanguage('pt-BR');
  await settle();
  ok(card!.textContent?.includes('Aguardando:'), 'pt-BR waiting label is displayed');
  ok(card!.textContent?.includes('A promoção aguarda'), 'pt-BR quiescence reason is displayed');
  useLanguage('en');
  await settle();

  // Start mission and verify live refresh
  await start(root);
  const initialCalls = callCount;
  status = {
    current: 'A · 111122223333',
    next: 'B · 777788889999',
    lastGood: 'A · 111122223333',
    waiting: 'projects.state.running',
  };
  emit({ ...view(2), state: 'running' });
  await settle();

  ok(callCount > initialCalls, 'mission updates trigger a live local update refresh');
  const activeCard = root.querySelector('.cost-pace-panel [data-testid="local-build"]');
  ok(activeCard, 'active mission displays build card in side panel');
  ok(activeCard!.textContent?.includes('B · 777788889999'), 'live slot update reflects new next slot');
  ok(activeCard!.textContent?.includes('Running'), 'live waiting reason reflects running state');
}));

async function start(root: HTMLElement) {
  const field = root.querySelector<HTMLTextAreaElement>('#mission-goal')!;
  field.value = 'Review goal'; field.dispatchEvent(new Event('input', { bubbles: true }));
  root.querySelector<HTMLFormElement>('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await settle();
}

check('projects review: unrelated and stale events cannot replace the selected mission or steal focus', () => withProjects(async (root, emit) => {
  const field = root.querySelector<HTMLTextAreaElement>('#mission-goal')!;
  field.focus();
  emit({ ...view(99), missionId: 'another-mission' });
  await settle();
  ok(root.querySelector('#mission-goal'), 'unrelated event must not replace the start card');
  equal(document.activeElement, field);
  await start(root);
  emit({ ...view(3), goal: 'Newest goal' });
  emit({ ...view(2), goal: 'Stale goal' });
  await settle();
  equal(root.querySelector('.m-goal')?.textContent, 'Newest goal');
  emit({ ...view(99), missionId: 'another-mission', goal: 'Wrong project' });
  await settle();
  equal(root.querySelector('.m-goal')?.textContent, 'Newest goal');
}));

check('projects review: switching projects clears the old mission and pending enrollment', () => withProjects(async (root, _emit, change) => {
  await start(root);
  change({ id: 'second-project', name: 'Second project', path: 'C:/fixture/second' });
  await settle();
  ok(root.querySelector('#mission-goal'), 'new project must show its own start card');
  equal(root.querySelector('h2')?.textContent, 'Second project');
}));

check('projects review: refreshing the same project object preserves its active mission', () => withProjects(async (root, _emit, change) => {
  await start(root); change({ ...demo }); await settle();
  ok(root.querySelector('.mission-board'), 'a list refresh must not enroll again and reset the board');
}));

check('projects review: failed grant keeps goal, displays refusal, and never starts work', () => withProjects(async (root) => {
  let starts = 0;
  api.projectGrant = async () => { throw 'reason.grant.none'; };
  api.missionStart = async () => { starts++; return 'wrong'; };
  await start(root);
  equal(starts, 0);
  equal(root.querySelector<HTMLTextAreaElement>('#mission-goal')?.value, 'Review goal');
  ok(root.querySelector('[role="alert"]')?.textContent?.includes('grant'), 'refusal is visible');
}));

check('projects T5: a refused role shows the T2 reason key and keeps the goal', () => withProjects(async (root) => {
  api.missionStart = async () => { throw 'reason.project.restrictionsUnproven'; };
  await start(root);
  ok(root.querySelector('[role="alert"]')?.textContent?.includes('reason.project.restrictionsUnproven'));
  equal(root.querySelector<HTMLTextAreaElement>('#mission-goal')?.value, 'Review goal');
  ok(!root.querySelector('.mission-board'));
}));

check('projects review: mission state is localized and unknown cap is never invented', () => withProjects(async (root) => {
  await start(root);
  equal(root.querySelector('.badge')?.textContent?.trim(), 'Running');
  ok(root.querySelector('.cost-cap')?.textContent?.includes('Unknown'), 'missing cap is unknown');
  equal(formatCap(null), 'projects.cost.unknown', 'formatting must not invent a cap');
}));

check('projects review: reopening restores the persisted mission checkpoint', async () => {
  localStorage.setItem('faber.project.mission.' + demo.id, 'review-mission');
  await withProjects(async (root) => {
    ok(root.querySelector('.mission-board'), 'reload restores the mission instead of offering duplicate work');
    equal(root.querySelector('.m-goal')?.textContent, 'Review goal');
  });
});

check('projects review: a delayed enrollment cannot replace another project grant', () => withProjects(async (root, _emit, change) => {
  let resolve!: (value: Awaited<ReturnType<typeof api.projectEnroll>>) => void;
  api.projectEnroll = (id) => id === 'slow' ? new Promise((done) => { resolve = done; }) : Promise.resolve({ projectId: id, repo: 'fast-repo', base: 'fast', mode: 'seguro', revision: 7, selfUpdate: false });
  change({ ...demo, id: 'slow', name: 'Slow' }); await settle();
  change({ ...demo, id: 'fast', name: 'Fast' }); await settle();
  resolve({ projectId: 'slow', repo: 'wrong-repo', base: 'wrong', mode: 'seguro', revision: 1, selfUpdate: true }); await settle();
  ok(root.querySelector('.p-meta')?.textContent?.includes('fast-repo'));
  ok(!root.querySelector('.p-meta')?.textContent?.includes('wrong-repo'));
}));

check('projects review: every task state has keyboard reachable evidence and honest missing checks', () => withProjects(async (root, emit) => {
  await start(root);
  emit({ ...view(2), tasks: ['ready', 'running', 'review', 'done', 'blocked'].map((state, index) => ({ id: String(index), title: `Task ${state}`, state: state as MissionView['tasks'][number]['state'], detail: { scope: ['ui/Projects.tsx'], proofs: ['screen'], artifacts: [] } })) });
  await settle();
  equal(root.querySelectorAll('.task-card').length, 5);
  const summary = root.querySelector<HTMLElement>('.task-card summary');
  ok(summary, 'task evidence must be behind a disclosure');
  ok(summary.getBoundingClientRect().height >= 24, 'the disclosure has a 24 px keyboard/pointer target');
  summary.focus(); equal(document.activeElement, summary);
  summary.click(); await settle();
  ok(root.querySelector('details')?.textContent?.includes('ui/Projects.tsx'));
  ok(root.querySelector('details')?.textContent?.includes('No engine-verified evidence'));
  ok(root.querySelector('.task-col.done')?.textContent?.includes('Unverified'));
}));

check('projects review: a fresh snapshot keeps decision typing and open task details in place', () => withProjects(async (root, emit) => {
  await start(root);
  const next: MissionView = { ...view(2), decisions: [{ id: 'open-choice', kind: 'direction', question: 'Pick a direction', options: [] }], tasks: [{ id: 'stable-task', title: 'Original title', state: 'running', detail: { scope: [], proofs: [], artifacts: [] } }] };
  emit(next); await settle();
  const field = root.querySelector<HTMLInputElement>('.ny-custom input')!;
  field.value = 'My unfinished answer'; field.dispatchEvent(new Event('input', { bubbles: true })); field.focus();
  emit(JSON.parse(JSON.stringify({ ...next, revision: 3 })) as MissionView); await settle();
  equal(document.activeElement, field, 'background snapshots must preserve the same input node and focus');
  equal(field.value, 'My unfinished answer');
  const summary = root.querySelector<HTMLElement>('.task-card summary')!;
  summary.click(); summary.focus(); await settle();
  emit(JSON.parse(JSON.stringify({ ...next, revision: 4, tasks: [{ ...next.tasks[0], title: 'Updated title' }] })) as MissionView); await settle();
  equal(document.activeElement, summary, 'stable task identity preserves keyboard focus');
  ok(root.querySelector('details')?.open, 'evidence stays open');
  ok(root.querySelector('.tc-title')?.textContent?.includes('Updated title'), 'the retained row still receives new data');
}));

check('projects review: valid decision IDs cannot read inherited answer-map properties', () => withProjects(async (root, emit) => {
  await start(root);
  for (const [index, id] of ['constructor', '__proto__', 'toString'].entries()) {
    emit({ ...view(index + 2), decisions: [{ id, kind: 'direction', question: 'An open choice', options: [] }] });
    await settle();
    const field = root.querySelector<HTMLInputElement>('.ny-custom input')!;
    equal(field.value, '', `${id} starts with an empty personal answer`);
    field.value = 'My answer'; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
    equal(field.value, 'My answer');
  }
}));

check('projects review: IME composition cannot submit an unfinished decision answer', () => withProjects(async (root, emit) => {
  await start(root);
  emit({ ...view(2), decisions: [{ id: 'ime-choice', kind: 'direction', question: 'An open choice', options: [] }] }); await settle();
  const field = root.querySelector<HTMLInputElement>('.ny-custom input')!;
  field.value = 'Unfinished text'; field.dispatchEvent(new Event('input', { bubbles: true }));
  const original = api.missionAnswer; let answers = 0;
  api.missionAnswer = async () => { answers++; return view(3); };
  try {
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', isComposing: true, bubbles: true })); await settle();
    equal(answers, 0, 'composition Enter belongs to the input method');
  } finally { api.missionAnswer = original; }
}));

check('projects review: missing build slots stay explicit; quota freshness is separate from own cost', () => withProjects(async (root, emit) => {
  await start(root);
  emit({ ...view(2), pace: [{ vendor: 'codex', window: 'seven_day', used: 0.31, room: 0.04, readAt: 1791000000 }] });
  await settle();
  ok(root.querySelector('.build-card'), 'unknown build slots must remain visible');
  ok(root.querySelector('.pace-item')?.textContent?.includes('Reading:'), 'actual reading time is visible');
  ok(root.querySelector('.cost-card')?.textContent?.includes('Unknown'));
  ok(root.querySelector('.pace-card')?.textContent?.includes('31%'));
  ok(root.querySelector('.pace-card')?.textContent?.includes('4%'));
}));

check('projects review AS-4: rendered composer gap is 64 px at both heights', () => withProjects(async (root) => {
  const field = root.querySelector<HTMLTextAreaElement>('#mission-goal')!;
  for (const text of ['', 'Three\nlines\nhere']) {
    field.value = text; field.dispatchEvent(new Event('input', { bubbles: true })); await settle(300);
    const composer = root.querySelector('.composer')!.getBoundingClientRect();
    const preceding = root.querySelector('.composer-container')!.previousElementSibling!.getBoundingClientRect();
    ok(Math.abs(composer.top - preceding.bottom - 64) < 0.01, 'real 64 px gap, including flex gap');
  }
}));

const settle = (ms: number = 30) => new Promise((resolve) => setTimeout(resolve, ms));

// AS-1: Panel motion vs reduced motion
check('projects AS-1: the real board has intermediate frames; reduced and none disable travel', () => withProjects(async (root, emit) => {
  root.dataset.motion = 'full';
  await start(root);
  const board = root.querySelector<HTMLElement>('.mission-board')!;
  const animations = board.getAnimations();
  if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
    ok(animations.length > 0, 'the board must animate, never pass when no animation exists');
    const animation = animations[0];
    animation.pause();
    animation.currentTime = Number(animation.effect!.getComputedTiming().duration) / 2;
    await new Promise(requestAnimationFrame);
    const opacity = parseFloat(getComputedStyle(board).opacity);
    ok(opacity > 0 && opacity < 1, 'a real intermediate frame is visible');
    animation.finish();
  }
  for (const motion of ['reduced', 'none']) {
    root.dataset.motion = motion;
    emit({ ...view(10 + (motion === 'none' ? 1 : 0)), state: 'paused' });
    await settle(250);
    equal(board.getAnimations().length, 0, `${motion} disables travel`);
    equal(getComputedStyle(root.querySelector('button')!).transitionDuration, '0s');
  }
}));

// AS-2: Composer growth
check('projects AS-2: composer auto-grows with wrapped text, caps at 40vh with scroll, and shrinks when cleared', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:600px;height:600px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);

  const project: Project = { id: 'as2-p', name: 'AS2 Project', path: 'C:/temp/as2' };
  const dispose = render(() => createComponent(Projects, { project, vendors: ['codex'], now: Date.now() }), root);

  try {
    await settle(40);
    const field = root.querySelector<HTMLTextAreaElement>('#mission-goal');
    ok(field !== null, 'mission goal textarea must be rendered on start card');
    const initialHeight = field.clientHeight;
    equal(initialHeight, 60, 'empty composer textarea has base 60px height');

    // Input multiline/wrapped text
    field.value = 'Fix the bug in the parser module.\n'.repeat(5);
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await settle(40);
    ok(field.clientHeight > initialHeight, 'multiline text expands composer height');

    // Huge text to test 40vh cap
    field.value = 'Long task description text that wraps over many lines. '.repeat(200);
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await settle(40);
    ok(field.clientHeight <= innerHeight * 0.4 + 2, 'height caps within 40vh');
    ok(field.scrollHeight > field.clientHeight, 'overflow produces internal scroll');

    // Clear text
    field.value = '';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await settle(40);
    equal(field.clientHeight, initialHeight, 'clearing restores initial height');
  } finally {
    dispose();
    root.remove();
  }
});

// AS-3: Breadcrumb encoding in all five locales
check('projects AS-3: all five locales reject mojibake in the rendered breadcrumb character ›', async () => {
  const originalOpen = api.open;
  const originalClock = api.setClock;
  api.open = async () => null;
  api.setClock = async () => {};

  const effects = { ambient: false, texture: false, glow: false };
  const project: Project = { id: 'as3-p', name: 'AS3 Project', path: 'C:/temp/as3' };
  const initial: AppState = {
    vendors: [], models: [], quotas: [], version: '0.2.0', running: { revision: 0, items: [] },
    projects: [project],
    conversations: [],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: {
      vars: themes.css.base, scheme: 'dark', motion: 'none', effects, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion: 'none', effects },
    },
  };

  try {
    for (const locale of LANGUAGES) {
      useLanguage(locale.code);
      localStorage.setItem('faber.place', JSON.stringify({ area: 'project', projectId: project.id }));
      const root = document.createElement('div');
      root.style.cssText = 'position:fixed;inset:0';
      root.dataset.motion = 'none';
      for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
      document.body.append(root);

      const dispose = render(() => createComponent(App, { initial: { ...initial, settings: { ...initial.settings, language: locale.code } }, inbox: { take: () => {} } }), root);
      try {
        await settle(30);
        equal(document.documentElement.lang, locale.code, 'the breadcrumb check must actually render the chosen locale');
        const crumb = root.querySelector('.crumb')!;
        ok(crumb !== null, `breadcrumb must render for locale ${locale.code}`);
        const sep = crumb.querySelector('.sep');
        equal(sep?.textContent?.trim(), '›', `separator in ${locale.code} must be single character ›`);
        ok(!crumb.textContent?.includes('â€º'), `mojibake â€º must not appear in locale ${locale.code}`);
      } finally {
        dispose();
        root.remove();
      }
    }
  } finally {
    api.open = originalOpen;
    api.setClock = originalClock;
    useLanguage('en');
    localStorage.removeItem('faber.place');
  }
});

// AS-4: 64 px thread/composer gap
check('projects AS-4: rendered gap above the composer is 64 px per R104', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:800px;height:700px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);

  const project: Project = { id: 'as4-p', name: 'AS4 Project', path: 'C:/temp/as4' };
  const dispose = render(() => createComponent(Projects, { project, vendors: ['claude'], now: Date.now() }), root);

  try {
    await settle(40);
    const container = root.querySelector<HTMLElement>('.composer-container');
    ok(container !== null, 'composer container must be rendered');
    const previous = container.previousElementSibling!.getBoundingClientRect();
    const composer = container.querySelector('.composer')!.getBoundingClientRect();
    ok(Math.abs(composer.top - previous.bottom - 64) < 0.01, 'rendered gap must be 64 px per R104');
  } finally {
    dispose();
    root.remove();
  }
});

// Start card presence choices
check('projects: start card offers the two presence choices with parked install explanation', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:700px;height:700px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);

  const project: Project = { id: 'p-presence', name: 'Presence Test', path: 'C:/temp/presence' };
  const dispose = render(() => createComponent(Projects, { project, vendors: ['claude'], now: Date.now() }), root);

  try {
    await settle(30);
    const buttons = [...root.querySelectorAll<HTMLButtonElement>('#presence-choice ~ .seg-ctl button')];
    equal(buttons.length, 2, 'two presence choices must be offered');
    equal(buttons[0]?.getAttribute('aria-pressed'), 'true', 'continueWithout is active by default');

    // Click findTools
    buttons[1]?.click();
    await settle(20);
    equal(buttons[1]?.getAttribute('aria-pressed'), 'true');
    const note = root.querySelector('[role="note"]');
    ok(note?.textContent?.includes('parked'), 'parked notice is displayed when findTools is chosen');
  } finally {
    dispose();
    root.remove();
  }
});

// Honest receipts & unverified display
check('projects: honest receipts show unverified and not run without fake claims', () => {
  const doneTask = { id: 't1', title: 'Task 1', state: 'done' as const };
  const statusDone = taskReceiptStatus(doneTask);
  equal(statusDone.textKey, 'projects.receipt.unverified', 'done task without proof receipt reports unverified');
  ok(statusDone.isHonest, 'status must be marked honest');

  const readyTask = { id: 't2', title: 'Task 2', state: 'ready' as const };
  const statusReady = taskReceiptStatus(readyTask);
  equal(statusReady.textKey, 'projects.receipt.notRun', 'ready task reports not run');
});

// Cost formatting
check('projects: null own cost shows unknown and never zero', () => {
  const unknownCost = formatCost(null);
  equal(unknownCost.isUnknown, true);
  equal(unknownCost.display, 'projects.cost.unknown');

  const zeroCost = formatCost(0);
  equal(zeroCost.isUnknown, false);
  equal(zeroCost.display, '$0.00');

  const normalCost = formatCost(1_500_000);
  equal(normalCost.isUnknown, false);
  equal(normalCost.display, '$1.50');
});

// Goal persistence
check('projects: goal text persists across refusal or failed start', () => {
  const pid = 'persist-test-proj';
  const goalText = 'Implement feature X cleanly';
  saveGoal(pid, goalText);
  equal(recallGoal(pid), goalText, 'saved goal text must round-trip through recallGoal');
  saveGoal(pid, '');
  equal(recallGoal(pid), '', 'clearing goal removes it');
});
