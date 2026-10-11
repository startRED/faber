import themes from '../../src-tauri/src/themes.json';
import { createLoop, shouldRun } from '../../ui/Ambient.tsx';
import type { Appearance, Look, SavedSettings } from '../../ui/api.ts';
import { applyLook, effectiveMotion, inOrder, previewer, saveAndApply, switchTheme, watchSystem, windowHidden } from '../../ui/appearance.ts';
import { en } from '../../ui/locales/en.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// A look with every token of the base theme, as the shell would send it.
const sala = (over: Partial<Look> = {}): Look => ({
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
function keepRoot(): () => void {
  const root = document.documentElement;
  const style = root.style.cssText;
  const data = { ...root.dataset };
  return () => {
    root.style.cssText = style;
    for (const key of Object.keys(root.dataset)) delete root.dataset[key];
    Object.assign(root.dataset, data);
  };
}

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

check('appearance: the effect loop runs only at full motion, with the switch on and the window visible', () => {
  const on = { motion: 'full', ambient: true, hidden: false } as const;
  ok(shouldRun(on));
  ok(!shouldRun({ ...on, hidden: true }), 'a hidden window');
  ok(!shouldRun({ ...on, motion: 'reduced' }), 'reduced motion');
  ok(!shouldRun({ ...on, motion: 'none' }), 'no motion');
  ok(!shouldRun({ ...on, ambient: false }), 'the ambient switch off');

  let next: FrameRequestCallback | undefined;
  let drawn = 0;
  let cancelled = 0;
  const loop = createLoop(
    () => (drawn += 1),
    24,
    (step) => ((next = step), 7),
    () => ((next = undefined), (cancelled += 1)),
  );
  loop.run(true);
  ok(loop.running(), 'the loop starts');
  next?.(performance.now() + 100);
  equal(drawn, 1);
  loop.run(false);
  ok(!loop.running(), 'the loop stops');
  equal(cancelled, 1);
  equal(next, undefined, 'no frame stays queued');
  loop.run(false);
  equal(cancelled, 1, 'stopping twice cancels once');
});

check('appearance: the loop holds its rate on a 60 Hz screen and goes back to counting after a stall', () => {
  let next: FrameRequestCallback | undefined;
  let drawn = 0;
  const loop = createLoop(
    () => (drawn += 1),
    24,
    (step) => ((next = step), 1),
    () => undefined,
  );
  loop.run(true);
  const start = performance.now();
  // One second of frames 1000/60 ms apart; each reset of the clock to the frame it drew on would give 20, not 24.
  for (let frame = 1; frame <= 60; frame += 1) next?.(start + (1000 / 60) * frame);
  ok(drawn >= 23 && drawn <= 25, `${drawn} draws in a second`);
  const before = drawn;
  next?.(start + 5000);
  equal(drawn, before + 1, 'a frame after a long stall draws once');
  next?.(start + 5000 + 1000 / 60);
  equal(drawn, before + 1, 'and the next frame, a sixtieth later, waits');
  loop.run(false);
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

check('appearance: hiding the window sets the hidden signal and the class that pauses the effects, and showing it clears both', () => {
  const root = document.documentElement;
  const stop = watchSystem();
  const hide = (value: boolean) => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => value });
    document.dispatchEvent(new Event('visibilitychange'));
  };
  try {
    hide(true);
    ok(windowHidden(), 'the signal follows the window');
    ok(root.classList.contains('hidden-win'), 'the class that pauses the CSS animations');
    hide(false);
    ok(!windowHidden(), 'the signal clears');
    ok(!root.classList.contains('hidden-win'), 'the class clears');
  } finally {
    Reflect.deleteProperty(document, 'hidden');
    root.classList.remove('hidden-win');
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
