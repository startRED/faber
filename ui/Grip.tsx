import { onCleanup } from 'solid-js';
import { motion } from './appearance.ts';
import { type Edge, fit, type Limits } from './layout.ts';
import { millis, remeasure } from './motion.ts';

/** The edge of the sidebar or the running column that the person drags to give it more or less room (docs/design/layout.md).
 * A focusable separator: the arrow keys move it by 16 px (48 with Shift), Home and End go to its limits, a double click or
 * Enter puts the usual width back. While a drag runs, the window's pointer events move a CSS variable of the shell, one
 * frame at a time; nothing is measured, and the width is stored once when the drag ends. */
export function Grip(props: { edge: Edge; label: string; value: number; limits: Limits; fallback: number; onChange: (px: number) => void }) {
  let el: HTMLDivElement | undefined;
  let frame = 0;
  let abort: (() => void) | undefined;
  // The column's edge is on its left: moving the pointer left makes it wider. The sidebar's is on its right.
  const grow = props.edge === 'side' ? 1 : -1;
  const paint = (px: number) => el!.closest<HTMLElement>('.shell')?.style.setProperty(`--${props.edge}-w`, `${px}px`);
  const stop = () => {
    cancelAnimationFrame(frame);
    frame = 0;
  };
  const commit = (px: number) => {
    paint(px);
    props.onChange(px);
    // The lists that move things when the layout changes measure again from here, so the next change starts where this one ended.
    remeasure();
  };
  onCleanup(() => (stop(), abort?.()));

  const down = (e: PointerEvent) => {
    if (e.button !== 0 || !e.isPrimary) return;
    e.preventDefault();
    stop();
    const from = props.value;
    const limits = props.limits;
    const x0 = e.clientX;
    const root = document.documentElement;
    let px = from;
    const move = (m: PointerEvent) => {
      px = fit(from + grow * (m.clientX - x0), limits);
      frame ||= requestAnimationFrame(() => ((frame = 0), paint(px)));
    };
    const end = () => {
      window.removeEventListener('pointermove', move);
      for (const type of ['pointerup', 'pointercancel']) window.removeEventListener(type, end);
      el!.removeEventListener('lostpointercapture', end);
      abort = undefined;
      root.classList.remove('sizing');
      el!.classList.remove('drag');
      stop();
      commit(px);
    };
    abort = end;
    root.classList.add('sizing');
    el!.classList.add('drag');
    // The pointer is kept by the handle, so a drag that leaves the window still ends where it is let go. The window hears the
    // events either way, so a pointer the browser cannot capture still drags.
    try {
      el!.setPointerCapture(e.pointerId);
    } catch {
      // not an active pointer
    }
    window.addEventListener('pointermove', move);
    for (const type of ['pointerup', 'pointercancel']) window.addEventListener(type, end);
    el!.addEventListener('lostpointercapture', end);
  };

  const reset = () => {
    stop();
    const from = props.value;
    const to = fit(props.fallback, props.limits);
    if (from === to) return props.onChange(to);
    if (motion() !== 'full') return commit(to);
    // A reset is rare and travels a short way: the width glides there instead of jumping.
    const ms = millis(el!, '--dur-ui', 220);
    const t0 = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - t0) / ms);
      if (k < 1 && motion() === 'full') {
        paint(from + (to - from) * (1 - (1 - k) ** 3));
        frame = requestAnimationFrame(step);
        return;
      }
      frame = 0;
      commit(to);
    };
    frame = requestAnimationFrame(step);
  };

  const key = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const step = (e.shiftKey ? 48 : 16) * grow;
    const to =
      e.key === 'ArrowRight' ? props.value + step
      : e.key === 'ArrowLeft' ? props.value - step
      : e.key === 'Home' ? props.limits.min
      : e.key === 'End' ? props.limits.max
      : undefined;
    if (e.key === 'Enter') {
      e.preventDefault();
      reset();
    } else if (to !== undefined) {
      e.preventDefault();
      stop();
      commit(fit(to, props.limits));
    }
  };

  return (
    <div
      ref={(node) => (el = node)}
      class={`grip ${props.edge}`}
      role="separator"
      aria-orientation="vertical"
      aria-label={props.label}
      title={props.label}
      aria-valuenow={props.value}
      aria-valuemin={props.limits.min}
      aria-valuemax={props.limits.max}
      tabindex="0"
      onPointerDown={down}
      onDblClick={reset}
      onKeyDown={key}
    />
  );
}
