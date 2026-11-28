import { batch } from 'solid-js';

/** Past this long without a frame (a hidden window draws none), what waits goes anyway, so nothing is held for good. */
const LATE_MS = 100;

/**
 * Hands events to `deliver` once per frame, in the order they came and in one batch, so a reply streaming 30 updates a
 * second, beside the running list and the queue, repaints the screen once per frame and not once per event (W1,
 * 2026-10-31). A window that draws no frames still gets them within `LATE_MS`.
 */
export function createFrameFeed<T>(
  deliver: (event: T) => void,
  frame: (run: () => void) => number = requestAnimationFrame,
  cancel: (id: number) => void = cancelAnimationFrame,
) {
  let waiting: T[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;
  // The frame asked for, cancelled when the timer goes first, so a hidden window never piles up callbacks.
  let asked: number | undefined;
  const flush = () => {
    clearTimeout(timer);
    timer = undefined;
    if (asked !== undefined) cancel(asked);
    asked = undefined;
    if (!waiting.length) return;
    const events = waiting;
    waiting = [];
    batch(() => {
      for (const event of events) deliver(event);
    });
  };
  return {
    push(event: T) {
      waiting.push(event);
      if (asked !== undefined) return;
      asked = frame(flush);
      timer = setTimeout(flush, LATE_MS);
    },
    /** What waits goes now. */
    flush,
  };
}
