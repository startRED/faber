import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type AgentsView, type AppState, type MissionView, type PermissionMode, type Project } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { fromEngine, lang, LANGUAGES, useLanguage } from '../../ui/i18n.ts';
import { Projects } from '../../ui/Projects.tsx';
import { ProjectsSection } from '../../ui/ProjectSettings.tsx';
import { formatCap, formatCost, recallGoal, saveGoal, taskReceiptStatus } from '../../ui/projects.ts';
import { dirtyCount, kept } from '../../ui/unsent.ts';
import { check, equal, ok } from './check.ts';

const demo: Project = { id: 'review-project', name: 'Review project', path: 'C:/fixture/review' };
const view = (revision = 1): MissionView => ({ missionId: 'review-mission', revision, state: 'running', mode: 'normal', gitMode: 'seguro', goal: 'Review goal', tasks: [], cost: { ownMicros: null, capMicros: null, complete: false }, pace: [], decisions: [] });

let agentsEmit: (v: AgentsView) => void = () => {};
async function withProjects(run: (root: HTMLElement, emit: (v: MissionView) => void, change: (p: Project) => void) => Promise<void>, before?: () => void, permissions: PermissionMode = 'ask') {
  const originals = { ...api };
  useLanguage('en');
  Object.assign(api, { localUpdateState: async () => null });
  let listener: (v: MissionView) => void = () => {};
  api.onAgents = async (fn) => { agentsEmit = fn; return () => {}; };
  api.projectEnroll = async (id) => ({ projectId: id, repo: demo.path, base: 'base', mode: 'seguro', revision: 1, selfUpdate: false });
  api.projectGrant = async () => ({ repo: demo.path, mode: 'seguro', revision: 1, grantedAt: 1, selfUpdate: false });
  api.projectProof = async () => ({ commands: ['npm test'], declared: ['npm test'], source: 'agents', confirmed: true, changed: false });
  api.missionStart = async () => 'review-mission';
  api.missionState = async () => view();
  api.onMission = async (fn) => { listener = fn; return () => {}; };
  before?.();
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const [project, setProject] = createSignal(demo);
  const dispose = render(() => createComponent(Projects, { get project() { return project(); }, vendors: [], now: Date.now(), permissions }), root);
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

check('projects: outside the development channel a mission board has no local build card', () => withProjects(async (root, emit) => {
  api.localUpdateState = async () => null;
  window.dispatchEvent(new Event('focus'));
  await settle();
  await start(root);
  emit({ ...view(2), state: 'running' });
  await settle();
  ok(root.querySelector('.cost-pace-panel'), 'the mission board is shown');
  equal(root.querySelector('[data-testid="local-build"]'), null, 'no build card for a build Faber does not make');
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

check('projects T5: a refused role says so in words, never as its raw key, and keeps the goal', () => withProjects(async (root) => {
  api.missionStart = async () => { throw 'reason.project.restrictionsUnproven'; };
  await start(root);
  const alert = root.querySelector('[role="alert"]')?.textContent ?? '';
  ok(alert.includes(fromEngine('reason.project.restrictionsUnproven')) && !alert.includes('reason.project'), `the refusal reads as a sentence: ${alert}`);
  equal(root.querySelector<HTMLTextAreaElement>('#mission-goal')?.value, 'Review goal');
  ok(!root.querySelector('.mission-board'));
}));

check('projects review: mission state is localized and unknown cap is never invented', () => withProjects(async (root) => {
  await start(root);
  equal(root.querySelector('[data-testid="mission-state"]')?.textContent?.trim(), 'Running');
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

check('projects review: on the development channel unknown build slots stay explicit; quota freshness is separate from own cost', () => withProjects(async (root, emit) => {
  api.localUpdateState = async () => ({});
  window.dispatchEvent(new Event('focus'));
  await settle();
  await start(root);
  emit({ ...view(2), pace: [{ vendor: 'codex', window: 'seven_day', used: 0.31, room: 0.04, readAt: 1791000000 }] });
  await settle();
  ok(root.querySelector('.build-card'), 'unknown build slots must remain visible');
  ok(root.querySelector('.pace-item')?.textContent?.includes('Reading:'), 'actual reading time is visible');
  ok(root.querySelector('.cost-card')?.textContent?.includes('Unknown'));
  ok(root.querySelector('.pace-card')?.textContent?.includes('31%'));
  ok(root.querySelector('.pace-card')?.textContent?.includes('4%'));
}));

check('projects review AS-4: the goal composer keeps a 32 px gap under the lead at both heights', () => withProjects(async (root) => {
  const field = root.querySelector<HTMLTextAreaElement>('#mission-goal')!;
  for (const text of ['', 'Three\nlines\nhere']) {
    field.value = text; field.dispatchEvent(new Event('input', { bubbles: true })); await settle(300);
    const composer = root.querySelector('.composer')!.getBoundingClientRect();
    const preceding = root.querySelector('.lead')!.getBoundingClientRect();
    ok(Math.abs(composer.top - preceding.bottom - 32) < 0.01, 'the same gap as the new conversation, the 64 px of R104 belongs to the thread');
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
check('projects AS-4: rendered gap above the goal composer is 32 px, the 64 px of R104 belongs to the thread', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:800px;height:700px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);

  const project: Project = { id: 'as4-p', name: 'AS4 Project', path: 'C:/temp/as4' };
  const dispose = render(() => createComponent(Projects, { project, vendors: ['claude'], now: Date.now() }), root);

  try {
    await settle(40);
    const lead = root.querySelector<HTMLElement>('.lead');
    ok(lead !== null, 'the lead must be rendered');
    const composer = root.querySelector('.composer')!.getBoundingClientRect();
    ok(Math.abs(composer.top - lead.getBoundingClientRect().bottom - 32) < 0.01, 'rendered gap must be 32 px');
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
check('projects: null own cost shows unknown and never zero, and a known one is written the way the language writes it', () => {
  const before = lang();
  useLanguage('en');
  const unknownCost = formatCost(null);
  equal(unknownCost.isUnknown, true);
  equal(unknownCost.display, 'projects.cost.unknown');

  const zeroCost = formatCost(0);
  equal(zeroCost.isUnknown, false);
  equal(zeroCost.display, '$0.00');

  const normalCost = formatCost(1_500_000);
  equal(normalCost.isUnknown, false);
  equal(normalCost.display, '$1.50');
  useLanguage('pt-BR');
  equal(formatCost(1_500_000).display, 'US$ 1,50');
  equal(formatCap(2_000_000), 'US$ 2,00');
  useLanguage(before);
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

// R137: a mission starts only on checks the owner confirmed. The screen asks once, just in time, and keeps every other
// setting in Settings, Projects.
const declared = ['npm run lint', 'cargo t'];
const proofView = (confirmed: boolean, commands = declared) => ({ commands, declared, source: 'agents' as const, confirmed, changed: false, setup: ['npm ci', 'cargo fetch'] });
const click = async (el: Element | null | undefined) => { ok(el, 'the element to click is on the screen'); (el as HTMLElement).click(); await settle(); };
const typeGoal = async (root: HTMLElement) => {
  const field = root.querySelector<HTMLTextAreaElement>('#mission-goal')!;
  field.value = 'Review goal'; field.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
};

check('projects: in Full access and Auto the start confirms the checks by itself and starts, with no sheet', async () => {
  for (const mode of ['dontAsk', 'auto'] as const) {
    let confirmed = false;
    let started = 0;
    await withProjects(async (root) => {
      const status = root.querySelector('[data-testid="p-status"]')?.textContent ?? '';
      ok(status.includes('allowed by your permission mode'), `the status line says the mode covers them: ${status}`);
      await typeGoal(root);
      await start(root);
      equal(root.querySelector('[data-testid="proof-sheet"]'), null, `${mode}: no sheet`);
      ok(confirmed, `${mode}: the declared checks were confirmed`);
      equal(started, 1, `${mode}: the mission started`);
    }, () => {
      api.projectProof = async () => proofView(confirmed);
      api.projectProofConfirm = async (_id, commands) => { confirmed = true; return proofView(true, commands); };
      api.missionStart = async () => { started += 1; return 'review-mission'; };
    }, mode);
  }
});

check('projects: the first start on unconfirmed checks opens a sheet that shows every command at once, and Confirm starts', () => {
  let confirmed = false;
  let started = 0;
  return withProjects(async (root) => {
    for (const gone of ['input[role="switch"]', '.seg-ctl', '[data-testid="proof-commands"]', '[data-testid="proof-sheet"]'])
      equal(root.querySelector(gone), null, `the start screen has no ${gone}`);
    const status = root.querySelector('[data-testid="p-status"]')?.textContent ?? '';
    ok(status.includes('2 checks read from AGENTS.md') && status.includes('confirm when you start'), `one quiet status line: ${status}`);
    await typeGoal(root);
    const submit = root.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    ok(!submit.disabled, 'the start is not disabled: it leads to the sheet');
    await start(root);
    equal(started, 0, 'unconfirmed checks never start a mission');
    const sheet = root.querySelector('[data-testid="proof-sheet"]');
    ok(sheet, 'the start asks for the checks');
    ok(sheet!.textContent?.includes('2 checks read from AGENTS.md'), `the sheet says how many and from where: ${sheet!.textContent}`);
    equal(sheet!.querySelector('button.more'), null, 'nothing in the sheet is collapsed: no confirming what is hidden');
    const list = sheet!.querySelector('[data-testid="proof-commands"]')?.textContent ?? '';
    ok(list.includes('npm run lint') && list.includes('cargo t'), `the sheet shows the commands before Confirm: ${list}`);
    ok(sheet!.querySelector('[data-testid="proof-setup"]')?.textContent?.includes('npm ci'), 'and the setup that runs first');
    const box = sheet!.querySelector<HTMLElement>('.proof-cmds')!;
    ok(['auto', 'scroll'].includes(getComputedStyle(box).overflowY) && getComputedStyle(box).maxHeight !== 'none', 'a long list scrolls inside the sheet');
    await click(sheet!.querySelector('[data-testid="proof-confirm"]'));
    ok(confirmed, 'the list the owner saw is confirmed');
    equal(started, 1, 'and the mission starts from there');
  }, () => {
    api.projectProof = async () => proofView(confirmed);
    api.projectProofConfirm = async (_id, commands, setup, shown) => {
      confirmed = true;
      equal(shown.join('|'), declared.join('|'), 'the declaration the owner saw is sent, so a changed file is refused');
      equal(commands.join('|'), declared.join('|'), 'the declared list is what gets confirmed');
      equal(setup.join('|'), 'npm ci|cargo fetch', 'the setup list the owner saw is what gets confirmed');
      return proofView(true, commands);
    };
    api.missionStart = async () => { started += 1; return 'review-mission'; };
  });
});

check('projects: checks that return after the owner switched project never reach the other project', () => {
  let release: (v: ReturnType<typeof proofView>) => void = () => {};
  let sent = 0;
  return withProjects(async (root, _emit, change) => {
    await typeGoal(root);
    await start(root);
    ok(!root.querySelector('[data-testid="proof-sheet"]'), 'project A is still reading its checks');
    change({ id: 'second-project', name: 'Second', path: 'C:/fixture/second' });
    await settle();
    release(proofView(false, ['rm-everything']));
    await settle();
    equal(root.querySelector('[data-testid="proof-sheet"]'), null, "A's late answer does not open a sheet on B");
    ok(!root.textContent?.includes('rm-everything'), "A's commands are nowhere on B's screen");
    equal(sent, 0, 'and nothing was confirmed');
  }, () => {
    let reads = 0;
    api.projectProof = (id) => {
      reads += 1;
      // Reads 1 (A opens) and 3 (B opens) are immediate; read 2 is A's start, which stays pending until after the switch.
      if (id === 'review-project' && reads === 2) return new Promise((resolve) => { release = resolve; });
      return Promise.resolve(proofView(false));
    };
    api.projectProofConfirm = async () => { sent += 1; return proofView(true); };
  });
});

check('projects: when the declaration changes after the checks were confirmed, Start refreshes and opens the sheet on the new list', () => {
  let current = proofView(true);
  let started = 0;
  let refused = 0;
  return withProjects(async (root) => {
    await typeGoal(root);
    // The file changes while the screen stays open: the cached proof still says confirmed.
    current = { ...proofView(false, ['npm run lint', 'npm run e2e']), declared: ['npm run lint', 'npm run e2e'], changed: true };
    await start(root);
    equal(refused, 1, 'the engine was asked once and refused');
    equal(started, 0, 'the engine refused, so no mission');
    const sheet = root.querySelector('[data-testid="proof-sheet"]');
    ok(sheet, 'the refusal opens the sheet');
    ok(sheet!.querySelector('[data-testid="proof-commands"]')?.textContent?.includes('npm run e2e'), 'with the new commands');
    ok(!root.querySelector('[role="alert"]')?.textContent?.includes('Confirm the project'), 'and no error line repeats the refusal');
  }, () => {
    api.projectProof = async () => current;
    api.missionStart = async () => { refused += 1; throw 'reason.proof.unconfirmed'; };
  });
});

check('projects: a confirmation the engine refuses because the files moved on reloads and shows the new list', () => {
  let current = proofView(false);
  return withProjects(async (root) => {
    await typeGoal(root);
    await start(root);
    current = proofView(false, ['npm run lint', 'cargo t', 'npm run e2e']);
    await click(root.querySelector('[data-testid="proof-confirm"]'));
    ok(root.querySelector('[data-testid="proof-commands"]')?.textContent?.includes('npm run e2e'), 'the sheet shows what the project declares now');
    ok(root.querySelector('[data-testid="proof-sheet"]'), 'and stays open');
  }, () => {
    api.projectProof = async () => current;
    api.projectProofConfirm = async () => { throw 'reason.proof.declaredChanged'; };
  });
});

check('projects: confirmed checks never ask again, and the status says the project is ready', () => {
  let started = 0;
  return withProjects(async (root) => {
    equal(root.querySelector('[data-testid="p-status"]')?.textContent, 'Ready for missions');
    await start(root);
    equal(root.querySelector('[data-testid="proof-sheet"]'), null, 'no sheet once confirmed');
    equal(started, 1);
  }, () => {
    api.projectProof = async () => proofView(true);
    api.missionStart = async () => { started += 1; return 'review-mission'; };
  });
});

check('projects: a start that cannot happen yet says why, beside the button', () => withProjects(async (root) => {
  await typeGoal(root);
  const submit = root.querySelector<HTMLButtonElement>('button[type="submit"]')!;
  ok(submit.disabled, 'the start waits while the project is read');
  const why = root.querySelector('[data-testid="p-status"]');
  equal(why?.textContent, 'Reading the project…', 'the reason is on the screen');
  equal(submit.getAttribute('aria-describedby'), why!.id, 'and tied to the button');
  equal(why!.parentElement, submit.parentElement, 'next to it');
}, () => {
  api.projectEnroll = () => new Promise(() => {});
}));

// Settings, Projects: one closed entry per project; inside, one short line per option and the rest behind Learn more.
async function withProjectSettings(mock: () => void, run: (root: HTMLElement) => Promise<void>) {
  const originals = { ...api };
  mock();
  useLanguage('en');
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:700px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(ProjectsSection, { projects: [demo, { id: 'p2', name: 'Second', path: 'C:/fixture/second' }] }), root);
  try { await settle(); await run(root); }
  finally { dispose(); root.remove(); Object.assign(api, originals); }
}

check('settings projects: entries and every Learn more start closed, and closed parts are not in the page', () => {
  let reads = 0;
  return withProjectSettings(() => { api.projectProof = async () => { reads++; return proofView(false); }; }, async (root) => {
    equal(root.querySelectorAll('.pset').length, 2, 'one entry per project');
    equal(root.querySelector('.pset-body'), null, 'every entry starts closed');
    equal(reads, 0, 'a closed entry reads nothing');
    await click(root.querySelector('.pset-head'));
    const body = root.querySelector('.pset-body')!;
    ok(body.querySelector('[data-testid="pset-checks"]')?.textContent?.includes('2 checks read from AGENTS.md · not confirmed yet'), 'the checks in one line');
    ok(body.querySelector('[data-testid="proof-commands"]'), 'unconfirmed checks show their commands: Confirm never acts on a hidden list');
    const more = [...body.querySelectorAll<HTMLButtonElement>('button.more')];
    ok(more.length >= 4, 'every option keeps its long text behind a disclosure');
    equal(more.some((b) => b.textContent === 'View commands'), false, 'no View while the list is pending: it is already shown');
    ok(more.every((b) => b.getAttribute('aria-expanded') === 'false'), 'all closed');
    equal(body.querySelector('.pset-line > p.hint'), null, 'no long text shows until asked');
  });
});

check('settings projects: the owner edits the checks, one command per line, and confirms the edited list', () => {
  let sent: string[] = [];
  return withProjectSettings(() => {
    api.projectProof = async () => proofView(false);
    api.projectProofConfirm = async (_id, commands) => { sent = commands; return proofView(true, commands); };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    await click(root.querySelector('[data-testid="proof-edit"]'));
    const box = root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!;
    box.value = ['cargo fmt --check', '', '  npm test  '].join(String.fromCharCode(10));
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await click(root.querySelector('[data-testid="proof-save"]'));
    equal(sent.join('|'), 'cargo fmt --check|npm test', 'blank lines go and lines are trimmed');
    ok(root.querySelector('[data-testid="pset-checks"]')?.textContent?.includes('Confirmed'), 'the edited list is confirmed');
  });
});

check('settings projects: Confirm is never pressable while the list or the setup is hidden', () => {
  let proof = proofView(false);
  return withProjectSettings(() => {
    api.projectProof = async () => proof;
    api.projectProofConfirm = async (_id, commands) => { proof = proofView(true, commands); return proof; };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    const body = root.querySelector('.pset-body')!;
    ok(body.querySelector('[data-testid="proof-confirm"]'), 'unconfirmed checks offer Confirm');
    ok(body.querySelector('[data-testid="proof-commands"]')?.textContent?.includes('npm run lint'), 'with every command on screen');
    ok(body.querySelector('[data-testid="proof-setup"]')?.textContent?.includes('npm ci'), 'and the setup that runs first');
    await click(body.querySelector('[data-testid="proof-edit"]'));
    ok(body.querySelector('[data-testid="proof-save"]'), 'editing offers Save');
    ok(body.querySelector('[data-testid="proof-setup"]')?.textContent?.includes('cargo fetch'), 'with the setup on screen beside the draft');
    await click([...body.querySelectorAll('button')].find((b) => b.textContent === 'Cancel'));
    await click(body.querySelector('[data-testid="proof-confirm"]'));
    equal(body.querySelector('[data-testid="proof-confirm"]'), null, 'confirmed: no Confirm');
    equal(body.querySelector('[data-testid="proof-commands"]'), null, 'and the calm view folds the list');
    await click([...body.querySelectorAll('button.more')].find((b) => b.textContent === 'View commands'));
    ok(body.querySelector('[data-testid="proof-commands"]'), 'View shows it again');
  });
});

check('settings projects: a refusal swaps the draft for the refreshed list and setup before Save is possible again', () => {
  let current = proofView(false);
  let sent: string[][] = [];
  return withProjectSettings(() => {
    api.projectProof = async () => current;
    api.projectProofConfirm = async (_id, commands, setup, shown) => {
      if (shown.join('|') !== current.declared.join('|')) throw 'reason.proof.declaredChanged';
      sent = [commands, setup, shown];
      return proofView(true, commands);
    };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    await click(root.querySelector('[data-testid="proof-edit"]'));
    const box = root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!;
    box.value = 'draft A';
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    current = { ...proofView(false, ['npm run evil']), declared: ['npm run evil'], setup: ['npm ci --ignore-scripts=false'] };
    await click(root.querySelector('[data-testid="proof-save"]'));
    equal(sent.length, 0, 'the refused save recorded nothing');
    equal(root.querySelector('[data-testid="proof-save"]'), null, 'Save is gone after a refusal');
    equal(root.querySelector('[data-testid="proof-draft"]'), null, 'and so is the stale draft');
    ok(root.querySelector('[data-testid="proof-commands"]')?.textContent?.includes('npm run evil'), 'the refreshed list is on screen');
    ok(root.querySelector('[data-testid="proof-setup"]')?.textContent?.includes('ignore-scripts'), 'with the refreshed setup');
    await click(root.querySelector('[data-testid="proof-edit"]'));
    equal(root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!.value, 'npm run evil', 'Edit starts again from the refreshed list');
    await click(root.querySelector('[data-testid="proof-save"]'));
    equal(sent[2].join('|'), 'npm run evil', 'what is saved is the declaration that was shown');
    equal(sent[1].join('|'), 'npm ci --ignore-scripts=false', 'with the setup that was shown');
  });
});

check('settings projects: a refusal for invalid commands that comes with a moved declaration still drops the draft', () => {
  let current = proofView(false);
  return withProjectSettings(() => {
    api.projectProof = async () => current;
    api.projectProofConfirm = async () => { throw 'reason.proof.command'; };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    await click(root.querySelector('[data-testid="proof-edit"]'));
    const box = root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!;
    box.value = 'npm test && npm run lint';
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    current = { ...proofView(false, ['npm run evil']), declared: ['npm run evil'] };
    await click(root.querySelector('[data-testid="proof-save"]'));
    equal(root.querySelector('[data-testid="proof-draft"]'), null, 'the draft is gone, whatever the error was');
    ok(root.querySelector('[data-testid="proof-commands"]')?.textContent?.includes('npm run evil'), 'the new list is on screen');
    equal(kept('proof.' + demo.id), '', 'and the stale draft is not kept for a reopen');
  });
});

check('settings projects: the self-update switch saves the grant with a fresh revision', () => {
  let granted: unknown[] = [];
  return withProjectSettings(() => {
    api.projectProof = async () => proofView(true);
    api.projectEnroll = async (id) => ({ projectId: id, repo: demo.path, base: 'base', mode: 'seguro', revision: 5, selfUpdate: false });
    api.projectGrant = async (...args) => { granted = args; return { repo: demo.path, mode: 'seguro', revision: 5, grantedAt: 1, selfUpdate: true }; };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    const box = root.querySelector<HTMLInputElement>('input[role="switch"]')!;
    box.click();
    await settle();
    equal(JSON.stringify(granted), JSON.stringify([demo.id, 'seguro', true, 5]));
    ok(box.checked, 'the switch stays on');
  });
});

check('projects home: text sits only on the faint veil of the black hole, scrolled or not', async () => {
  const originals = { ...api };
  useLanguage('en');
  Object.assign(api, { localUpdateState: async () => null });
  api.projectEnroll = async (id) => ({ projectId: id, repo: demo.path, base: 'base', mode: 'seguro', revision: 1, selfUpdate: false });
  api.projectProof = async () => proofView(false);
  api.onMission = async () => () => {};
  api.onAgents = async () => () => {};
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:1600px;height:560px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const recent = [1, 2, 3, 4].map((n) => ({ id: `c${n}`, title: `Earlier talk ${n}`, area: 'project' as const, projectId: demo.id, updatedAt: Date.now() - n * 60_000 }));
  const dispose = render(() => createComponent(Projects, { project: demo, vendors: ['claude'], now: Date.now(), projects: [demo], recent, running: [], onOpen: () => {} }), root);
  try {
    await settle(300);
    // Open the sheet and its list, so the column runs past the bottom and scrolls.
    await typeGoal(root);
    await start(root);
    await settle(200);
    equal(root.querySelectorAll('.p-resume .rrow').length, 3, 'at most three to pick up');
    const canvas = root.querySelector<HTMLCanvasElement>('canvas.amb-hole')!;
    ok(canvas.width > 0, 'Dark Room draws the black hole');
    const ctx = canvas.getContext('2d')!;
    const all = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    let lit = 0;
    for (let i = 3; i < all.length; i += 4) if (all[i]! > 20) lit++;
    ok(lit > 1000, `the field is drawn, ${lit} lit pixels`);
    const clearOfText = (moment: string) => {
      const box = canvas.getBoundingClientRect();
      const scale = canvas.width / box.width;
      const walker = document.createTreeWalker(root.querySelector('.projects-stage')!, NodeFilter.SHOW_TEXT);
      const range = document.createRange();
      let seen = 0;
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        if (!node.textContent?.trim()) continue;
        // The glyphs, not the box that holds them: a row runs the column's full width past its last letter, as the veil's
        // own edge does not (ui/Hole.tsx).
        range.selectNodeContents(node);
        const r = range.getBoundingClientRect();
        const [x0, y0] = [Math.max(0, Math.floor((r.left - box.left) * scale)), Math.max(0, Math.floor((r.top - box.top) * scale))];
        const [x1, y1] = [Math.min(canvas.width, Math.ceil((r.right - box.left) * scale)), Math.min(canvas.height, Math.ceil((r.bottom - box.top) * scale))];
        if (x1 <= x0 || y1 <= y0) continue;
        seen++;
        const px = ctx.getImageData(x0, y0, x1 - x0, y1 - y0).data;
        let worst = 0;
        for (let i = 3; i < px.length; i += 4) worst = Math.max(worst, px[i]!);
        // Behind the column the veil keeps a faint part of the light (tests/ui/hole.check.ts); at its edge, where the light
        // starts to come up, a glyph under text still stays under a third of its brightness, so the text reads easily.
        ok(worst <= 77, `${moment}: "${node.textContent.trim().slice(0, 30)}" sits on the bright part of the black hole (${worst})`);
      }
      ok(seen > 10, `${moment}: the texts were measured (${seen})`);
    };
    clearOfText('at the top');
    const stage = root.querySelector<HTMLElement>('.projects-stage')!;
    stage.scrollTop = stage.scrollHeight;
    await settle(100);
    ok(stage.scrollTop > 0, 'the column scrolls');
    clearOfText('scrolled');
  } finally {
    dispose();
    root.remove();
    Object.assign(api, originals);
  }
});

check('projects: a running mission shows its step, role, vendor and cost so far, then its report', () => withProjects(async (root, emit) => {
  await start(root);
  emit({
    ...view(2),
    cost: { ownMicros: 250_000, capMicros: 2_000_000, complete: false },
    progress: { step: 'write', number: 4, total: 7, role: 'writer', vendor: 'claude', model: 'claude-haiku-5-5' },
  });
  await settle();
  const line = root.querySelector('[data-testid="mission-progress"]')?.textContent ?? '';
  ok(line.includes('Step 4 of 7') && line.includes('Writing the change'), `the step is shown: ${line}`);
  ok(line.includes('Writer') && line.includes('Claude'), `role and vendor are shown, the vendor by name: ${line}`);
  ok(line.includes('$0.25'), `the cost so far is shown: ${line}`);
  ok(!root.querySelector('[data-testid="mission-report"]'), 'no report while it runs');
  emit({
    ...view(3),
    state: 'done',
    report: { branch: 'faber/review-mission', head: '0123456789abcdef', checks: [{ id: 'proof-1', result: 'pass' }], seconds: 125 },
  });
  await settle();
  const report = root.querySelector('[data-testid="mission-report"]')?.textContent ?? '';
  ok(report.includes('faber/review-mission'), `the report names the branch: ${report}`);
  ok(report.includes('01234567') && !report.includes('0123456789abcdef'), 'the commit is shortened');
  ok(report.includes('proof-1') && report.includes('passed'), `the checks are listed: ${report}`);
  ok(report.includes('2 min 5 s'), `the duration is shown: ${report}`);
  ok(report.includes('Your own branch was not touched'), 'the report says the owner branch is safe');
  ok(!root.querySelector('[data-testid="mission-progress"]'), 'progress goes away when done');
}));

check('projects: a parked or blocked mission asks for the owner in the Needs you card, in English and pt-BR', () => withProjects(async (root, emit) => {
  let resumed = 0;
  api.missionResume = async () => { resumed += 1; return { ...view(4), state: 'running' as const }; };
  await start(root);
  emit({ ...view(2), state: 'blocked', reason: 'reason.proof.failed', needs: 'reason.proof.failed' });
  await settle();
  const card = root.querySelector('[data-testid="mission-needs"]');
  ok(card, 'a blocked mission shows a need');
  ok(card!.textContent?.includes('The checks failed on the finished change'), 'the reason reads as a sentence');
  ok(root.querySelector('.needs-you'), 'it uses the Needs you region');
  useLanguage('pt-BR');
  await settle();
  ok(root.querySelector('[data-testid="mission-needs"]')!.textContent?.includes('As verificações falharam'), 'the reason is hand-written in pt-BR');
  useLanguage('en');
  await settle();
  await click(root.querySelector('[data-testid="mission-needs"] button'));
  equal(resumed, 1, 'the card resumes the mission');
}));

check('projects: the check of the agents shows its time left, then an honest failure, and goes away when ready', () => withProjects(async (root) => {
  agentsEmit({ state: 'checking', step: 2, total: 7, vendor: 'codex', estimateSeconds: 180 });
  await settle();
  const checking = root.querySelector('[data-testid="agents-status"]')?.textContent ?? '';
  ok(checking.includes('Checking the agents') && checking.includes('~180 s') && checking.includes('step 2 of 7'), `the wait is shown: ${checking}`);
  agentsEmit({ state: 'failed', step: 0, total: 7, estimateSeconds: 0, detail: 'codex-native: native write escaped workspace' });
  await settle();
  const failed = root.querySelector('[data-testid="agents-status"]');
  ok(failed?.textContent?.includes('no mission was started') && failed.textContent.includes('native write escaped workspace'), 'the failure carries the probe reason');
  equal(failed?.getAttribute('role'), 'alert');
  useLanguage('pt-BR');
  await settle();
  ok(root.querySelector('[data-testid="agents-status"]')?.textContent?.includes('nenhuma missão foi iniciada'), 'the failure is hand-written in pt-BR');
  useLanguage('en');
  agentsEmit({ state: 'ready', step: 7, total: 7, estimateSeconds: 0 });
  await settle();
  ok(!root.querySelector('[data-testid="agents-status"]'), 'nothing is shown once the agents are ready');
}));

check('projects review: a typed custom decision answer counts as unsent until it is cleared', () => withProjects(async (root, emit) => {
  await start(root);
  emit({ ...view(2), decisions: [{ id: 'draft-choice', kind: 'direction', question: 'Pick a direction', options: [] }] }); await settle();
  const before = dirtyCount();
  const field = root.querySelector<HTMLInputElement>('.ny-custom input')!;
  field.value = 'typed, not sent'; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  equal(dirtyCount(), before + 1, 'an unattended install must wait for it');
  field.value = ''; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  equal(dirtyCount(), before, 'an emptied field has nothing to lose');
}));

check('projects board: a paused mission says why in words, shows no raw key and can be left for a new mission', () => withProjects(async (root, emit) => {
  await start(root);
  emit({ ...view(2), state: 'paused', reason: 'reason.budget.calibration' });
  await settle();
  const board = root.querySelector('.mission-board')!.textContent ?? '';
  ok(!/\breason\.[\w.]+/.test(board), `no raw engine key on the board: ${board}`);
  ok(board.includes('still learning how much of your quota'), 'the calibration wait reads as a sentence');
  ok(board.includes('No tasks yet'), 'a board with no tasks says so in one line, not five empty columns');
  equal(root.querySelectorAll('.task-col').length, 0, 'empty task groups are not drawn');
  ok(!root.querySelector('.m-more')?.hasAttribute('open'), 'run mode, cost and build start folded away');
  await click(root.querySelector('[data-testid="mission-leave"]'));
  ok(root.querySelector('.mission-board'), 'a paused mission asks before it is left');
  equal(document.activeElement?.textContent, 'Stay', 'the question starts on the safe answer');
  await click(root.querySelector('[data-testid="mission-leave-yes"]'));
  ok(!root.querySelector('.mission-board'), 'leaving opens the start card');
  ok(root.querySelector('#mission-goal'), 'the goal box is back');
  equal(localStorage.getItem('faber.project.mission.' + demo.id), null, 'the board does not come back on reload');
}));

check('projects board: an unknown engine key never shows raw, in any language', () => {
  for (const { code } of LANGUAGES) {
    useLanguage(code);
    for (const key of ['reason.budget.someNewWait', 'reason.kit.someNewReason', 'error.someNewFailure']) ok(fromEngine(key) !== key, `${code} shows ${key} raw`);
  }
  useLanguage('en');
});
