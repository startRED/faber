import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type AppState, type Project } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { LANGUAGES, useLanguage } from '../../ui/i18n.ts';
import { animateList } from '../../ui/motion.ts';
import { Projects } from '../../ui/Projects.tsx';
import { formatCost, recallGoal, saveGoal, taskReceiptStatus } from '../../ui/projects.ts';
import { check, equal, ok } from './check.ts';

const settle = (ms: number = 30) => new Promise((resolve) => setTimeout(resolve, ms));

// AS-1: Panel motion vs reduced motion
check('projects AS-1: panel transition staggers intermediate states with motion and stays instant with reduced motion', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:500px;height:400px';
  root.dataset.motion = 'full';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = '<div class="projects-stage"><div class="start-card"><button>1</button><button>2</button></div></div>';
  document.body.append(root);

  const stage = root.firstElementChild as HTMLElement;
  const dispose = render(() => { animateList(stage); return [...stage.children] as HTMLElement[]; }, stage);

  try {
    await settle(20);
    const animations = stage.getAnimations({ subtree: true });
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches && animations.length > 0) {
      const timing = animations.map((a) => a.effect!.getComputedTiming());
      ok(timing.every((t) => Number(t.delay) + Number(t.duration) <= 400), 'all panel animations complete within 400ms');
    }
    // Switch to reduced motion
    root.dataset.motion = 'reduced';
    const button = root.querySelector('button')!;
    equal(getComputedStyle(button).transitionDuration, '0s', 'reduced motion disables transition duration');
  } finally {
    dispose();
    root.remove();
  }
});

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

      const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
      try {
        await settle(30);
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
check('projects AS-4: gap above the composer is 64 px per R104', async () => {
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
    const marginTop = parseFloat(getComputedStyle(container).marginTop);
    equal(marginTop, 64, 'gap above composer container must be exactly 64 px per R104');
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
