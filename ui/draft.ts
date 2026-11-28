import themes from '../src-tauri/src/themes.json';
import type { Appearance, Look } from './api.ts';

// The pure rules behind the Appearance section of Settings (ui/Appearance.tsx): the draft the person is working on, what
// it is compared with, and what the preview panel reads off a look. The screen never computes a colour: the shell does,
// and these functions only read its answers.

/** The factory look: Dark Room, the theme's own sizes and the system's motion. */
export const factory = (): Appearance => ({ base: themes.default });

// Keys in a fixed order, so two appearances that make the same choices read the same.
const canon = (value: unknown): unknown => {
  if (Array.isArray(value)) return value.map(canon);
  if (value === null || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([, v]) => v !== undefined)
      .sort(([a], [b]) => (a < b ? -1 : 1))
      .map(([k, v]) => [k, canon(v)]),
  );
};

/** Whether two appearances make the same choices. No appearance at all is the factory look. */
export function sameChoices(a: Appearance | undefined, b: Appearance | undefined): boolean {
  return JSON.stringify(canon(a ?? factory())) === JSON.stringify(canon(b ?? factory()));
}

/** What the controls show: the draft's own choices over the last answer's, which has every choice filled in from the
 * base. Between a change and its answer a control already shows the choice, and the fields the draft leaves out show
 * what the look gives them. */
export function shown(draft: Appearance, look: Look): Look['appearance'] {
  const own = Object.fromEntries(Object.entries(draft).filter(([, v]) => v !== undefined));
  return { ...look.appearance, ...own };
}

/** One of the three contrast readings of the preview panel: text, faint text and the accent, each on the background. */
export interface Reading {
  id: 'text' | 'faint' | 'accent';
  ratio: number;
  pass: boolean;
}

const READINGS = [
  ['text', 'ink'],
  ['faint', 'ink-3'],
  ['accent', 'accent'],
] as const;

/** The readings the look carries, in the order the panel shows them. */
export function readings(look: Look): Reading[] {
  return READINGS.flatMap(([id, ink]) => {
    const found = look.checks.find((c) => c.a === ink && c.b === 'bg');
    return found ? [{ id, ratio: found.ratio, pass: found.pass }] : [];
  });
}

/** The pairs of colours that are too close to read, as the look names them ("ink-3 / s3, ..."); empty when every pair
 * passes. A look with a failing pair cannot be applied. */
export const failingPairs = (look: Look): string =>
  look.checks
    .filter((c) => !c.pass)
    .map((c) => `${c.a} / ${c.b}`)
    .join(', ');

/** A colour typed by hand: `#rrggbb`, `rrggbb` or the short `#rgb`, in any case. The answer is `#rrggbb` in lower case,
 * or undefined when the text is not a colour. */
export function normalizeHex(text: string): string | undefined {
  const hex = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(text.trim())?.[1]?.toLowerCase();
  if (!hex) return undefined;
  return `#${hex.length === 3 ? [...hex].map((c) => c + c).join('') : hex}`;
}

type Tokens = Record<string, string>;
const css = themes.css as unknown as { base: Tokens; themes: Record<string, Tokens>; presets: Record<string, Tokens> };

/** The key of the words that name a ground. A look that keeps its own ground (High contrast) reports it as its own id, which
 * is not one of the five a person can pick; it is named by the look. */
export const groundKey = (id: string): string => (themes['user-controls'].grounds.includes(id) ? `look.ground.${id}` : `look.${id}.name`);

/** The background a ground gives, for its chip: the same colour its theme or preset uses. */
export const groundColor = (id: string): string => css.themes[id]?.bg ?? css.presets[id]?.bg ?? css.base.bg!;
