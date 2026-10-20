import { createEffect, createSignal, onCleanup, untrack, type Accessor } from 'solid-js';
import { motion, windowHidden } from './appearance.ts';
import { t } from './i18n.ts';

// The quiet life of empty and waiting screens (docs/design/looks.md, "Motion": empty states are the expressive tier). It runs
// only at full motion and only while the window is shown: reduced motion and none render none of it, and a hidden or
// minimized window has no timer left running.

/** What the idle composer of a new conversation suggests, in turn: the area's own work, never the other area's (the
 * owner, 2026-10-18: a project gets project hints). */
export const homeHints = (area: 'chat' | 'project'): string[] =>
  area === 'project'
    ? [t('composer.projectPlaceholder'), t('hint.project.2'), t('hint.project.3')]
    : [t('composer.chatPlaceholder'), t('hint.chat.2'), t('hint.chat.3')];

/** True while idle life may run. */
export const idleLife = (): boolean => motion() === 'full' && !windowHidden();

const [active, setActive] = createSignal(document.hasFocus());
window.addEventListener('focus', () => setActive(true));
window.addEventListener('blur', () => setActive(false));

/** True while the window has the keyboard. An inactive window draws no caret of its own, so the screen draws a calm one. */
export const windowActive = active;

/** A counter that goes up every `ms` while idle life runs. Its timer exists only then and ends with the owner. */
export function slowTick(ms: number, enabled: Accessor<boolean> = () => true): Accessor<number> {
  const [n, setN] = createSignal(0);
  createEffect(() => {
    if (!idleLife() || !enabled()) return;
    const timer = setInterval(() => setN((v) => v + 1), ms);
    onCleanup(() => clearInterval(timer));
  });
  return n;
}

/**
 * Runs `run` every `ms` while the window is shown. The timer does not exist while the window is hidden or minimized, and it
 * is made again when the window comes back, which runs `run` once at once because the time moved on while nobody looked.
 */
export function everyWhileShown(ms: number, run: () => void): void {
  let wasHidden = false;
  createEffect(() => {
    if (windowHidden()) {
      wasHidden = true;
      return;
    }
    if (wasHidden) {
      wasHidden = false;
      untrack(run);
    }
    const timer = setInterval(run, ms);
    onCleanup(() => clearInterval(timer));
  });
}
