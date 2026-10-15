import { createEffect, createSignal, onCleanup, type Accessor } from 'solid-js';
import { motion, windowHidden } from './appearance.ts';

// The quiet life of empty and waiting screens (docs/design.md, "Motion": empty states are the expressive tier). It runs
// only at full motion and only while the window is shown: reduced motion and none render none of it, and a hidden or
// minimized window has no timer left running.

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
