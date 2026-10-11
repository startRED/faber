import { createSignal } from 'solid-js';
import type { Appearance, Look, Motion, SavedSettings, Settings } from './api.ts';

// Applying a look (docs/design.md, "Looks"). The shell computes every look; this module only writes it to the page.

const systemReduces = matchMedia('(prefers-reduced-motion: reduce)');

const [applied, setApplied] = createSignal<Look>();
const [reduced, setReduced] = createSignal(systemReduces.matches);
const [hidden, setHidden] = createSignal(document.hidden);

/** The look on the page, once one was applied. */
export const appliedLook = applied;
/** The motion level that is in force: the user's, capped by the system. */
export const motion = (): Motion => effectiveMotion(applied()?.motion ?? 'full', reduced());
/** True while the window is hidden; effect loops stop. */
export const windowHidden = hidden;

/** A system that asks for less motion turns `full` into `reduced`; the other levels already ask for less. */
export const effectiveMotion = (level: Motion, systemReducesMotion: boolean): Motion => (level === 'full' && systemReducesMotion ? 'reduced' : level);

const on = (flag: boolean) => (flag ? 'on' : 'off');

/** Writes a look to `target`: the page itself, or an element that previews it. */
export function applyLook(look: Look, target: HTMLElement = document.documentElement): void {
  for (const [name, value] of Object.entries(look.vars)) target.style.setProperty(`--${name}`, value);
  target.style.colorScheme = look.scheme;
  target.dataset.base = look.appearance.base;
  target.dataset.motion = effectiveMotion(look.motion, reduced());
  target.dataset.ambient = on(look.effects.ambient);
  target.dataset.texture = on(look.effects.texture);
  target.dataset.glow = on(look.effects.glow);
  if (target === document.documentElement) setApplied(look);
}

/** Follows the system's motion setting and the window's visibility. A change in the system setting changes only the
 * motion level, never the rest of the look. Returns the function that stops following. */
export function watchSystem(): () => void {
  const onMotion = () => {
    setReduced(systemReduces.matches);
    const look = applied();
    if (look) document.documentElement.dataset.motion = effectiveMotion(look.motion, systemReduces.matches);
  };
  const onVisibility = () => {
    setHidden(document.hidden);
    // The CSS animations of the effects pause with the window, like the dust loop does.
    document.documentElement.classList.toggle('hidden-win', document.hidden);
  };
  systemReduces.addEventListener('change', onMotion);
  document.addEventListener('visibilitychange', onVisibility);
  return () => {
    systemReduces.removeEventListener('change', onMotion);
    document.removeEventListener('visibilitychange', onVisibility);
  };
}

const KEPT = ['textSize', 'density', 'motion'] as const;

/** Choosing a theme or preset keeps the text size, the density and the motion, and clears every other choice, so the
 * new theme shows as designed. */
export function switchTheme(current: Appearance | undefined, base: string): Appearance {
  const next: Appearance = { base };
  for (const key of KEPT) if (current?.[key] !== undefined) Object.assign(next, { [key]: current[key] });
  return next;
}

/** Saves a patch. Only when the save succeeded, and only when the patch changed the appearance, the look the shell
 * answers with is applied; a rejection leaves the page as it was. */
export async function saveAndApply(patch: Settings, save: (patch: Settings) => Promise<SavedSettings>): Promise<SavedSettings> {
  const saved = await save(patch);
  if (patch.appearance) applyLook(saved.look);
  return saved;
}

/** Runs jobs one after another. Saves go through it, so a choice is built from what the save before it stored and an
 * older answer can never land after a newer one. */
export function inOrder(): <T>(job: () => Promise<T>) => Promise<T> {
  let tail: Promise<unknown> = Promise.resolve();
  return (job) => {
    const run = tail.then(job);
    tail = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  };
}

/** Previews drafts. Each request carries a number that grows with every request sent; an answer is applied only if
 * its number is still the latest, so a quick run of changes can never end on an older draft. `cancel` makes every request
 * still on its way stale: the screen that sent them is gone, or the saved look is about to be applied. */
export function previewer(
  request: (draft: Appearance) => Promise<Look>,
  apply: (look: Look) => void,
  fail: (err: unknown) => void,
): ((draft: Appearance) => Promise<void>) & { cancel: () => void } {
  let latest = 0;
  const preview = async (draft: Appearance) => {
    const mine = ++latest;
    try {
      const look = await request(draft);
      if (mine === latest) apply(look);
    } catch (err) {
      if (mine === latest) fail(err);
    }
  };
  return Object.assign(preview, { cancel: () => void latest++ });
}
