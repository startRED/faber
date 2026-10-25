import themes from '../../src-tauri/src/themes.json';
import { breath, clearance, dustShadows, makeGrains } from '../../ui/Ambient.tsx';
import type { AppState, Appearance, Look, SavedSettings } from '../../ui/api.ts';
import { appliedLook, applyLook, effectiveMotion, inOrder, previewer, refreshed, saveAndApply, switchTheme, watchSystem, windowHidden } from '../../ui/appearance.ts';
import { en } from '../../ui/locales/en.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// A look with every token of the base theme, as the shell would send it.
export const sala = (over: Partial<Look> = {}): Look => ({
  vars: { ...themes.css.base },
  scheme: 'dark',
  motion: 'full',
  effects: { ambient: true, texture: true, glow: true },
  checks: [],
  notices: [],
  appearance: {
    base: 'sala',
    ground: 'sala',
    accent: '#f3f0e8',
    textFont: 'archivo',
    headingFont: 'narrow',
    textSize: 14,
    density: 1,
    corners: 1,
    motion: 'full',
    effects: { ambient: true, texture: true, glow: true },
  },
  ...over,
});

/** Puts `<html>` back the way it was when a check that applies a look onto the page is done. */
export function keepRoot(): () => void {
  const root = document.documentElement;
  const style = root.style.cssText;
  const data = { ...root.dataset };
  return () => {
    root.style.cssText = style;
    for (const key of Object.keys(root.dataset)) delete root.dataset[key];
    Object.assign(root.dataset, data);
  };
}

check('appearance: theme cross-fades use native snapshots, suppress stale previews and stay off in reduced motion', async () => {
  const restore = keepRoot();
  const saved = appliedLook();
  const original = document.startViewTransition;
  let calls = 0;
  let skips = 0;
  let update!: () => void;
  document.startViewTransition = (callback) => {
    calls++;
    let resolve!: () => void;
    const done = new Promise<void>((r) => { resolve = r; });
    let rejectReady!: () => void;
    const ready = new Promise<void>((_, reject) => { rejectReady = () => reject(new DOMException('Transition was skipped. skipTransition() called', 'AbortError')); });
    update = () => { if (typeof callback === 'function') void callback(); resolve(); };
    return { ready, updateCallbackDone: done, finished: done, skipTransition: () => { skips++; rejectReady(); } } as unknown as ViewTransition;
  };
  try {
    await applyLook(sala({ motion: 'none' }));
    await applyLook(sala());
    const warm = sala({ appearance: { ...sala().appearance, base: 'oficina' } });
    const pending = applyLook(warm);
    equal(calls, 1);
    await applyLook(sala({ motion: 'reduced' }));
    equal(skips, 1);
    update();
    await pending;
    await new Promise((resolve) => setTimeout(resolve, 0));
    equal(document.documentElement.dataset.base, 'sala', 'a snapshot from a closed preview never applies later');
    const before = calls;
    await applyLook({ ...warm, motion: 'reduced' });
    await applyLook(sala({ motion: 'none' }));
    equal(calls, before, 'reduced and none take no snapshots');
  } finally {
    document.startViewTransition = original;
    if (saved) await applyLook(saved);
    restore();
  }
});

check('appearance: a look writes every variable, the scheme and the flags on the element it is given', () => {
  const target = document.createElement('div');
  const root = document.documentElement.style.cssText;
  const look = sala({ scheme: 'light', motion: 'reduced', effects: { ambient: false, texture: true, glow: false } });
  applyLook(look, target);
  const names = Object.keys(look.vars);
  for (const name of ['frame', 'bg', 'accent', 'font-ui', 'dens', 'stage-shadow', 'amb-beam']) ok(names.includes(name), `the base theme should have --${name}`);
  for (const name of names) equal(target.style.getPropertyValue(`--${name}`), look.vars[name], `--${name}`);
  equal(target.style.colorScheme, 'light');
  equal(target.dataset.base, 'sala');
  equal(target.dataset.ambient, 'off');
  equal(target.dataset.texture, 'on');
  equal(target.dataset.glow, 'off');
  equal(target.dataset.motion, 'reduced');
  equal(document.documentElement.style.cssText, root, 'a preview must not touch the page');
});

check('appearance: the system preference caps the motion level and never raises it', () => {
  equal(effectiveMotion('full', true), 'reduced');
  equal(effectiveMotion('full', false), 'full');
  equal(effectiveMotion('reduced', true), 'reduced');
  equal(effectiveMotion('reduced', false), 'reduced');
  equal(effectiveMotion('none', true), 'none');
  equal(effectiveMotion('none', false), 'none');
});

check('appearance: a breath of the dust goes from 0 to 1 and back, eased in and out', () => {
  equal(breath(0, 6000), 0);
  equal(breath(3000, 6000), 0.5);
  equal(breath(6000, 6000), 1);
  equal(breath(12000, 6000), 0);
  ok(breath(600, 6000) < 0.01 && breath(5400, 6000) > 0.99, 'slow at both ends');
});

check('appearance: the dust is the same at every launch and never lies behind text', () => {
  const grains = makeGrains();
  equal(grains.length, 72);
  equal(JSON.stringify(makeGrains()), JSON.stringify(grains), 'a fixed seed');
  // The colour is the look's `dust` token: three channels, as the stylesheet writes them inside rgba().
  const free = dustShadows(grains, [], 0, '255,255,255');
  equal(free.length, 3, 'three twinkling layers');
  const box = { l: -1000, t: -1000, r: 1000, b: 1000 };
  ok(clearance([box], 0, 0) <= 0, 'a point inside a text block has no room');
  ok(dustShadows(grains, [box], 0, '255,255,255').every((layer) => layer === ''), 'a block over everything leaves no grain');
  ok(free.every((layer) => layer !== ''), 'with no text the dust is drawn');
  // The browser must take what was generated: a shadow it refuses is dropped and nothing is drawn.
  const dot = document.createElement('i');
  for (const layer of free) {
    ok(CSS.supports('box-shadow', layer), 'the browser accepts the shadows');
    dot.style.boxShadow = layer;
    ok(dot.style.boxShadow !== '', 'the shadows survive being set on an element');
  }
});

check('appearance: saves run in order, and a failed one does not stop the next', async () => {
  const turn = inOrder();
  const log: string[] = [];
  const gate: { open: () => void } = { open: () => undefined };
  const first = turn(async () => {
    log.push('first starts');
    await new Promise<void>((resolve) => (gate.open = resolve));
    log.push('first ends');
    return 1;
  });
  const second = turn(async () => {
    log.push('second starts');
    return 2;
  });
  await Promise.resolve();
  deepEqual(log, ['first starts'], 'the second waits for the first');
  gate.open();
  deepEqual([await first, await second], [1, 2]);
  deepEqual(log, ['first starts', 'first ends', 'second starts']);

  let thrown: unknown;
  const failing = turn(() => Promise.reject(new Error('disk')));
  const after = turn(() => Promise.resolve('still runs'));
  try {
    await failing;
  } catch (err) {
    thrown = err;
  }
  ok(thrown instanceof Error, 'the failure reaches its own caller');
  equal(await after, 'still runs');
});

check('appearance: hiding or blurring the window sets the class that pauses the effects, and showing it with the focus clears it', () => {
  const root = document.documentElement;
  let focused = true;
  document.hasFocus = () => focused;
  const stop = watchSystem();
  const hide = (value: boolean) => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => value });
    document.dispatchEvent(new Event('visibilitychange'));
  };
  try {
    hide(true);
    ok(windowHidden(), 'the signal follows the window');
    ok(root.classList.contains('away-win'), 'the class that pauses the CSS animations');
    hide(false);
    ok(!windowHidden(), 'the signal clears');
    ok(!root.classList.contains('away-win'), 'the class clears');
    focused = false;
    window.dispatchEvent(new Event('blur'));
    ok(root.classList.contains('away-win'), 'another app took the focus');
    focused = true;
    window.dispatchEvent(new Event('focus'));
    ok(!root.classList.contains('away-win'), 'the focus came back');
  } finally {
    Reflect.deleteProperty(document, 'hidden');
    Reflect.deleteProperty(document, 'hasFocus');
    root.classList.remove('away-win');
    stop();
  }
});

check('appearance: two previews answered in reverse order leave the screen on the later request', async () => {
  const calls: Array<{ resolve: (look: Look) => void; reject: (err: unknown) => void }> = [];
  const shown: string[] = [];
  const failed: unknown[] = [];
  const preview = previewer(
    () => new Promise<Look>((resolve, reject) => calls.push({ resolve, reject })),
    (look) => shown.push(look.appearance.base),
    (err) => failed.push(err),
  );
  const answer = (base: string) => sala({ appearance: { ...sala().appearance, base } });

  // The later request is answered first; the older answer arrives after it and is dropped.
  const run = [preview({ base: 'sala' }), preview({ base: 'oficina' })];
  calls[1]!.resolve(answer('oficina'));
  calls[0]!.resolve(answer('sala'));
  await Promise.all(run);
  deepEqual(shown, ['oficina']);

  // An older failure is dropped as well.
  const more = [preview({ base: 'bancada' }), preview({ base: 'manha' })];
  calls[3]!.resolve(answer('manha'));
  calls[2]!.reject({ key: 'error.look.pairFailed' });
  await Promise.all(more);
  deepEqual(shown, ['oficina', 'manha']);
  deepEqual(failed, []);

  // The latest failure is reported.
  const last = preview({ base: 'alto' });
  calls[4]!.reject({ key: 'error.look.pairFailed' });
  await last;
  deepEqual(failed, [{ key: 'error.look.pairFailed' }]);
});

check('appearance: a cancelled preview never applies or fails, and a later one still does', async () => {
  const calls: Array<{ resolve: (look: Look) => void; reject: (err: unknown) => void }> = [];
  const shown: string[] = [];
  const failed: unknown[] = [];
  const preview = previewer(
    () => new Promise<Look>((resolve, reject) => calls.push({ resolve, reject })),
    (look) => shown.push(look.appearance.base),
    (err) => failed.push(err),
  );
  const answer = (base: string) => sala({ appearance: { ...sala().appearance, base } });
  // The step is closed (or the saved look is about to apply) while two requests are on their way.
  const gone = [preview({ base: 'oficina' }), preview({ base: 'bancada' })];
  preview.cancel();
  calls[0]!.resolve(answer('oficina'));
  calls[1]!.reject({ key: 'error.look.pairFailed' });
  await Promise.all(gone);
  deepEqual(shown, []);
  deepEqual(failed, []);
  // The step opens again with its own previews.
  const next = preview({ base: 'bancada' });
  calls[2]!.resolve(answer('bancada'));
  await next;
  deepEqual(shown, ['bancada']);
});

check('appearance: a rejected save leaves the page as it was, and an accepted one applies the look it answers with', async () => {
  const restore = keepRoot();
  try {
    const root = document.documentElement;
    const before = { style: root.style.cssText, data: { ...root.dataset } };
    const refused = { key: 'error.settings.saveFailed' };
    let rejected: unknown;
    try {
      await saveAndApply({ appearance: { base: 'oficina' } }, () => Promise.reject(refused));
    } catch (err) {
      rejected = err;
    }
    equal(rejected, refused, 'the rejection reaches the caller');
    equal(root.style.cssText, before.style, 'the variables are untouched');
    deepEqual({ ...root.dataset }, before.data, 'the flags are untouched');

    const answer = (look: Look): SavedSettings => ({ settings: {}, look });
    const warm = sala({ vars: { ...themes.css.base, bg: '#121110' }, appearance: { ...sala().appearance, base: 'oficina' } });
    await saveAndApply({ language: 'es' }, () => Promise.resolve(answer(warm)));
    equal(root.style.cssText, before.style, 'a patch without appearance applies no look');
    await saveAndApply({ appearance: { base: 'oficina' } }, () => Promise.resolve(answer(warm)));
    equal(root.style.getPropertyValue('--bg'), '#121110');
    equal(root.dataset.base, 'oficina');
  } finally {
    restore();
  }
});

check('appearance: choosing a theme keeps the text size, the density and the motion, and clears the rest', () => {
  const chosen: Appearance = {
    base: 'sala',
    ground: 'manha',
    accent: '#2f7a62',
    textFont: 'plex',
    headingFont: 'book',
    textSize: 16,
    density: 0.8,
    corners: 0,
    motion: 'none',
    effects: { ambient: false, texture: false, glow: false },
    colors: { bg: '#101010' },
  };
  deepEqual(switchTheme(chosen, 'oficina'), { base: 'oficina', textSize: 16, density: 0.8, motion: 'none' });
  deepEqual(switchTheme(undefined, 'alto'), { base: 'alto' });
  deepEqual(switchTheme({ base: 'sala', accent: '#8fdcb8' }, 'bancada'), { base: 'bancada' });
});

check('appearance: every option the screen lists from themes.json has its text', () => {
  const controls = themes['user-controls'];
  const need = (key: string) => ok(key in en, `missing text ${key}`);
  for (const id of [...controls.themes, ...controls.presets]) [`look.${id}.name`, `look.${id}.desc`].forEach(need);
  for (const id of Object.keys(controls.accent.swatches)) need(`look.accent.${id}`);
  for (const id of controls.grounds) need(`look.ground.${id}`);
  for (const id of Object.keys(controls['text-fonts'])) need(`look.font.${id}`);
  for (const id of Object.keys(controls['heading-fonts'])) need(`look.heading.${id}`);
  for (const id of Object.keys(controls.motion)) [`look.motion.${id}`, `look.motion.${id}Hint`].forEach(need);
  for (const id of Object.keys(controls.effects)) need(`look.effect.${id}`);
  equal(controls.themes.length + controls.presets.length, 6);
});

check('refreshed: a state asked for before a save brings the rest back but never the old settings or look', () => {
  const state = (settings: AppState['settings'], look: Look, conversations: number): AppState => ({
    conversations: Array.from({ length: conversations }, (_, i) => ({ id: String(i), title: 't', area: 'chat', updatedAt: i })),
    projects: [],
    vendors: [],
    settings,
    look,
    models: [],
    running: { revision: 1, items: [] },
    quotas: [],
    version: '1',
  });
  const newer = state({ language: 'pt-BR' }, sala({ scheme: 'light' }), 1);
  const older = state({ language: 'en' }, sala(), 2);
  const merged = refreshed(newer, older);
  equal(merged.settings.language, 'pt-BR');
  equal(merged.look.scheme, 'light');
  equal(merged.conversations.length, 2, 'what the refresh owns is taken');
});
