import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Ambient } from '../../ui/Ambient.tsx';
import { api, type Project } from '../../ui/api.ts';
import { appliedLook, applyLook, watchSystem } from '../../ui/appearance.ts';
import { Composer } from '../../ui/Composer.tsx';
import { useLanguage } from '../../ui/i18n.ts';
import { Projects } from '../../ui/Projects.tsx';
import { keepRoot, sala } from './appearance.check.ts';
import { check, equal, ok } from './check.ts';

// The quiet life of the two homes (docs/design.md, "Motion"): a typing bar and hints in the idle composer, a light behind
// each home, and none of it with reduced motion, with no motion, or while the window is hidden.

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const HINTS = ['One', 'Two', 'Three'];

const systemReduces = matchMedia('(prefers-reduced-motion: reduce)');

/** Runs `body` with the page wearing a look of this motion level, and puts the page back after. A machine whose system asks
 * for reduced motion caps `full` at `reduced`, so the checks of full motion have nothing to see there and stand aside. */
async function withMotion(motion: 'full' | 'reduced' | 'none', body: () => Promise<void>) {
  if (motion === 'full' && systemReduces.matches) return;
  const restore = keepRoot();
  const saved = appliedLook();
  try {
    await applyLook(sala({ motion }));
    await body();
  } finally {
    if (saved) await applyLook(saved);
    restore();
  }
}

async function withComposer(hints: string[] | undefined, body: (root: HTMLElement, field: HTMLTextAreaElement) => Promise<void>) {
  const root = document.createElement('div');
  root.className = 'welcome-grid';
  root.style.cssText = 'position:fixed;left:0;top:0;width:640px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'The plain placeholder', hints,
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
  }), root);
  try {
    await settle();
    await body(root, root.querySelector('textarea')!);
  } finally {
    dispose();
    root.remove();
  }
}

check('idle: the empty composer of a home draws a typing bar and hints, and never two carets at once', () => withMotion('full', () => withComposer(HINTS, async (root, field) => {
  ok(root.querySelector('.idle-box'), 'the idle composer has its hints');
  equal(root.querySelectorAll('.g-hint').length, 3);
  equal(root.querySelectorAll('.g-hint.on').length, 1, 'one hint shows at a time');
  equal(getComputedStyle(field, '::placeholder').color, 'rgba(0, 0, 0, 0)', 'the hints stand in for the placeholder, so it is not drawn twice');
  const caret = () => root.querySelectorAll('.g-caret').length;
  // The window has the keyboard and the field has focus: the real caret blinks, and the screen draws none.
  window.dispatchEvent(new Event('focus'));
  field.dispatchEvent(new Event('focus'));
  await settle();
  equal(caret(), 0, 'a focused field in the front window shows its own caret only');
  // The window loses the keyboard: the real caret goes, so the calm one comes back.
  window.dispatchEvent(new Event('blur'));
  await settle();
  equal(caret(), 1, 'an inactive window still shows a blinking bar');
  ok(getComputedStyle(root.querySelector('.g-caret')!).animationName.includes('blink'), 'and it blinks');
  window.dispatchEvent(new Event('focus'));
  field.dispatchEvent(new Event('blur'));
  await settle();
  equal(caret(), 1, 'a field without focus shows the bar');
  // Typing removes all of it, and the placeholder comes back for the field.
  field.value = 'x';
  field.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
  equal(root.querySelector('.idle-box'), null, 'typed text leaves no hint and no bar');
  field.value = '';
  field.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
  ok(root.querySelector('.idle-box'), 'an emptied field idles again');
})));

check('idle: the hint changes slowly, once per tick, and its timer ends with the composer', () => withMotion('full', async () => {
  const set = window.setInterval;
  const clear = window.clearInterval;
  const timers = new Map<number, { callback: () => void; ms: number }>();
  let next = 9000;
  window.setInterval = ((callback: () => void, ms: number) => (timers.set(++next, { callback, ms }), next)) as unknown as typeof setInterval;
  window.clearInterval = ((id: number) => void timers.delete(id)) as typeof clearInterval;
  try {
    await withComposer(HINTS, async (root) => {
      const hint = [...timers.values()].find((t) => t.ms >= 6000);
      ok(hint, 'one timer rotates the hints');
      ok(hint.ms >= 6000, 'slowly: not more than once every few seconds');
      const shown = () => [...root.querySelectorAll('.g-hint')].findIndex((h) => h.classList.contains('on'));
      equal(shown(), 0);
      hint.callback();
      await settle();
      equal(shown(), 1, 'the next hint takes over');
    });
    equal([...timers.values()].filter((t) => t.ms >= 6000).length, 0, 'closing the composer ends the timer');
  } finally {
    window.setInterval = set;
    window.clearInterval = clear;
  }
}));

check('idle: reduced motion and no motion render no hints and no bar, only the plain placeholder', async () => {
  for (const level of ['reduced', 'none'] as const) {
    await withMotion(level, () => withComposer(HINTS, async (root, field) => {
      equal(root.querySelector('.idle-box'), null, `${level}: nothing idles`);
      equal(root.querySelector('.g-caret'), null, `${level}: no bar`);
      equal(field.placeholder, 'The plain placeholder');
      ok(getComputedStyle(field, '::placeholder').color !== 'rgba(0, 0, 0, 0)', `${level}: the placeholder is drawn`);
    }));
  }
});

check('idle: a hidden window renders no hints, has no timer left and pauses every CSS animation', () => withMotion('full', async () => {
  const root = document.documentElement;
  const stop = watchSystem();
  const hide = (value: boolean) => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => value });
    document.dispatchEvent(new Event('visibilitychange'));
  };
  const probe = document.createElement('span');
  probe.className = 'cursor';
  document.body.append(probe);
  try {
    await withComposer(HINTS, async (host) => {
      ok(host.querySelector('.idle-box'), 'shown: the composer idles');
      equal(getComputedStyle(probe).animationPlayState, 'running');
      hide(true);
      await settle();
      equal(host.querySelector('.idle-box'), null, 'hidden: no hint and no bar');
      equal(getComputedStyle(probe).animationPlayState, 'paused', 'hidden: even an animation the effects know nothing about stops');
      hide(false);
      await settle();
      ok(host.querySelector('.idle-box'), 'shown again: the composer idles again');
      equal(getComputedStyle(probe).animationPlayState, 'running');
    });
  } finally {
    probe.remove();
    Reflect.deleteProperty(document, 'hidden');
    root.classList.remove('hidden-win');
    stop();
  }
}));

check('idle: a look with no effects of its own gets a drifting light, off with the ambient switch and with reduced motion', async () => {
  const bare = (motion: 'full' | 'reduced', ambient: boolean) => {
    const look = sala({ motion });
    look.vars = { ...look.vars, 'amb-beam': 'none', 'amb-loom': 'none', 'amb-flow': 'none' };
    look.effects = { ambient, texture: true, glow: true };
    return look;
  };
  const restore = keepRoot();
  const saved = appliedLook();
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:500px';
  document.body.append(host);
  const dispose = render(() => createComponent(Ambient, { vendors: ['claude'], avoid: () => [], variant: 'chat' }), host);
  try {
    await applyLook(bare('full', true));
    await settle();
    const light = host.querySelector<HTMLElement>('.amb-drift')!;
    equal(getComputedStyle(light).display, 'block', 'a plain look has a light');
    // A system that asks for less motion caps the level, so the light stands still there (the reduced step below covers it).
    if (!systemReduces.matches) equal(getComputedStyle(light).animationName, 'drift');
    await applyLook(bare('full', false));
    await settle();
    equal(getComputedStyle(light).display, 'none', 'the ambient switch turns it off');
    await applyLook(bare('reduced', true));
    await settle();
    equal(getComputedStyle(light).animationName, 'none', 'reduced motion stops it');
    await applyLook(sala());
    await settle();
    equal(getComputedStyle(light).display, 'none', 'a look with its own effect does not add a second light');
  } finally {
    dispose();
    host.remove();
    if (saved) await applyLook(saved);
    restore();
  }
});

check('idle: a project home has no beam of light, the chat home keeps its own', async () => {
  const restore = keepRoot();
  const saved = appliedLook();
  const beam = async (variant: 'chat' | 'project') => {
    const host = document.createElement('div');
    host.style.cssText = 'position:fixed;left:0;top:0;width:1200px;height:600px';
    document.body.append(host);
    const dispose = render(() => createComponent(Ambient, { vendors: [], avoid: () => [], variant }), host);
    try {
      await settle();
      return [...host.querySelectorAll<HTMLElement>('.amb-beam, .amb-glow')].filter((el) => el.getClientRects().length > 0).length;
    } finally {
      dispose();
      host.remove();
    }
  };
  try {
    await applyLook(sala());
    ok((await beam('chat')) > 0, 'the chat home has its beam');
    equal(await beam('project'), 0, 'a project screen is a place to work: no beam and no glow');
  } finally {
    if (saved) await applyLook(saved);
    restore();
  }
});

const demo: Project = { id: 'idle-home', name: 'Idle home', path: 'C:/fixture/idle-home' };

async function withProjectHome(motion: 'full' | 'none', body: (root: HTMLElement) => Promise<void>) {
  if (motion === 'full' && systemReduces.matches) return;
  const originals = { ...api };
  useLanguage('en');
  Object.assign(api, { localUpdateState: async () => null });
  api.projectEnroll = async (id) => ({ projectId: id, repo: demo.path, base: 'base', mode: 'seguro', revision: 1, selfUpdate: false });
  api.onMission = async () => () => {};
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:1400px;height:800px';
  root.dataset.motion = motion;
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const other: Project = { id: 'other', name: 'Other project', path: 'C:/fixture/other' };
  const dispose = render(() => createComponent(Projects, {
    project: demo, vendors: ['claude'], now: Date.now(), projects: [demo, other],
    recent: [{ id: 'c1', title: 'An earlier talk', area: 'project', projectId: 'other', updatedAt: Date.now() - 60_000 }],
    running: [], onOpen: () => {}, onProject: () => {}, onAdd: () => {},
  }), root);
  try {
    await settle();
    await body(root);
  } finally {
    dispose();
    root.remove();
    Object.assign(api, originals);
  }
}

check('projects home: a project with no mission opens a home like the chat one, not a bare card', () => withProjectHome('full', async (root) => {
  const card = root.querySelector('.start-card')!;
  ok(card.classList.contains('welcome-grid'), 'it uses the structure of the new conversation');
  equal(root.querySelector('.hello')?.tagName, 'H1', 'a display greeting');
  ok(root.querySelector('.lead')?.textContent?.trim(), 'one calm line under it');
  ok(root.querySelector('.ambient.project'), 'its own light sits behind it');
  const composer = root.querySelector('.composer')!;
  // Only the goal lives here: the settings of a project are in Settings, Projects.
  for (const gone of ['input[role="switch"]', '.seg-ctl', '.proof-list', 'details', '.setup'])
    equal(root.querySelector(gone), null, `the project screen has no ${gone}`);
  equal(root.querySelector('h2')?.textContent, 'Idle home', 'the project is named');
  const resume = root.querySelector('.p-resume')!;
  ok(composer.compareDocumentPosition(resume) & Node.DOCUMENT_POSITION_FOLLOWING, 'what to pick up comes under the composer');
  equal(resume.querySelector('.rrow .t')?.textContent, 'An earlier talk', 'what to pick up from the projects');
  ok(![...root.querySelectorAll('.rrow .t')].some((e) => e.textContent === 'Other project'), 'the sidebar lists the projects, this screen does not');
  ok(root.querySelectorAll('.p-resume .rrow').length <= 3, 'at most three to pick up');
  ok(composer.getBoundingClientRect().top > root.querySelector('.lead')!.getBoundingClientRect().bottom, 'under the lead');
  const meta = root.querySelector<HTMLElement>('.p-meta')!;
  equal(meta.querySelectorAll('span').length, 1, 'the home names only the folder, not the revision or the author');
  ok(meta.querySelector('span')!.title.length >= meta.textContent!.length - 10, 'the full path is on hover');
  ok(root.querySelector('.composer .idle-box'), 'the idle composer has its hints');
}));

check('projects home: with no motion the home is as still as the chat one, and the field keeps its placeholder', () => withMotion('none', () => withProjectHome('none', async (root) => {
  equal(root.querySelector('.composer .idle-box'), null);
  ok(root.querySelector('#mission-goal')?.getAttribute('placeholder'), 'the plain placeholder');
  equal(root.querySelector('.hello')?.tagName, 'H1', 'the home itself stays');
})));
