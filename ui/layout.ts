import { type Accessor, createMemo, createSignal, onCleanup } from 'solid-js';

// The window widths where the shell changes (docs/design/layout.md). ui/styles.css repeats the same two numbers in its
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

/** The window's width in pixels, kept up to date while the calling component lives. */
export function createWindowWidth(): Accessor<number> {
  const [px, setPx] = createSignal(window.innerWidth);
  const onResize = () => setPx(window.innerWidth);
  window.addEventListener('resize', onResize);
  onCleanup(() => window.removeEventListener('resize', onResize));
  return px;
}

/** The width class of the window, kept up to date while the calling component lives. */
export function createWidth(): Accessor<Width> {
  const px = createWindowWidth();
  return createMemo(() => widthClass(px()));
}

// What the person may drag (docs/design/layout.md). The sidebar and the running column keep these widths, in pixels, and the
// stage beside them never gets narrower than STAGE_MIN: ui/styles.css clamps a docked column with the same numbers, so a
// window that shrinks after a drag is right before any script runs.
export type Edge = 'side' | 'run';
export interface Widths {
  side: number;
  /** Absent until the person drags the column: then it follows the window (`autoRun`). */
  run?: number;
}
export interface Limits {
  min: number;
  max: number;
}
export const SIDE: Limits & { fallback: number } = { min: 200, max: 420, fallback: 288 };
export const RUN: Limits = { min: 300, max: 560 };
export const STAGE_MIN = 440;
/** The reading column's width, which the running column never takes. */
export const READING = 760;

/** The widths an edge may take. A docked column leaves the stage STAGE_MIN; one over the stage may take its whole range. Where
 * the column docks, the sidebar leaves room for the column's minimum and the stage's. */
export function limitsOf(edge: Edge, win: number, side: number, docked: boolean): Limits {
  if (edge === 'side') return docked ? { ...SIDE, max: Math.max(SIDE.min, Math.min(SIDE.max, win - RUN.min - STAGE_MIN)) } : SIDE;
  return { min: RUN.min, max: docked ? Math.max(RUN.min, Math.min(RUN.max, win - side - STAGE_MIN)) : RUN.max };
}

/** A width brought into its limits, in whole pixels. */
export const fit = (px: number, { min, max }: Limits) => Math.round(Math.max(min, Math.min(max, px)));

/** The running column's usual width: the room the window leaves beside the reading column and 20 px of air on each side of it, 360 to 560 px. */
export const autoRun = (win: number, side: number) => fit(win - side - READING - 40, { min: 360, max: RUN.max });

/** The widths kept in storage, or the usual ones for anything that is not a width. */
export function savedWidths(raw: unknown): Widths {
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const pick = (value: unknown, limits: Limits) => (typeof value === 'number' && Number.isFinite(value) ? fit(value, limits) : undefined);
  return { side: pick(r.side, SIDE) ?? SIDE.fallback, run: pick(r.run, RUN) };
}
