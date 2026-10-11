import { type Accessor, createSignal, onCleanup } from 'solid-js';

// The window widths where the shell changes (docs/design.md, "Layout"). ui/styles.css repeats the same two numbers in its
// media queries, so the first paint is right before any script runs; a screen test checks both sides of each edge.
//   wide:   1,100 px and more. The sidebar and an open running column sit beside the stage.
//   mid:    900 to 1,099 px. The sidebar sits beside the stage; an open running column overlays it.
//   narrow: 720 to 899 px. The sidebar is behind a button; an open running column overlays the stage.
export type Width = 'wide' | 'mid' | 'narrow';
export const WIDE_FROM = 1100;
export const MID_FROM = 900;

export function widthClass(px: number): Width {
  return px >= WIDE_FROM ? 'wide' : px >= MID_FROM ? 'mid' : 'narrow';
}

/** The width class of the window, kept up to date while the calling component lives. */
export function createWidth(): Accessor<Width> {
  const [width, setWidth] = createSignal(widthClass(window.innerWidth));
  const onResize = () => setWidth(widthClass(window.innerWidth));
  window.addEventListener('resize', onResize);
  onCleanup(() => window.removeEventListener('resize', onResize));
  return width;
}
