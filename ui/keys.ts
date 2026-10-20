import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';
import { mac } from './WindowControls.tsx';

// Every keyboard shortcut Faber has, in the one place that defines them (parity K1). The handlers ask `pressed`, the
// sheet (ui/Shortcuts.tsx) and the tooltips read the same rows, so a key cannot work without being listed. A key that a
// control handles by itself (Enter, Esc, the arrows) is listed here with no `ctrl`, and its handler stays where it is.

/** A key cap: a letter or sign as it is printed, or one whose name each language says (Ctrl is Strg in German). */
type Cap = 'ctrl' | 'shift' | 'enter' | 'esc' | 'up' | 'down' | 'tab' | 'f10' | 'menukey' | (string & {});

export interface Shortcut {
  /** Where it works, the sheet's heading. */
  group: 'anywhere' | 'message';
  /** What it does. */
  label: Key;
  /** The caps in the order they are pressed. */
  caps: readonly Cap[];
  /** Another way to press it, shown after the first. */
  also?: readonly Cap[];
  /** The `KeyboardEvent.key` (lower case) for a shortcut with Ctrl (⌘ on a Mac), matched by `pressed`. */
  ctrl?: string;
  /** Shift and Alt do not matter (the keys that always took them: Ctrl+Shift+N opens a conversation too, and a comma
   * typed with Shift on some layouts still opens Settings). */
  loose?: true;
}

export const SHORTCUTS = {
  new: { group: 'anywhere', label: 'side.new', caps: ['ctrl', 'N'], ctrl: 'n', loose: true },
  palette: { group: 'anywhere', label: 'palette.label', caps: ['ctrl', 'K'], ctrl: 'k' },
  find: { group: 'anywhere', label: 'find.label', caps: ['ctrl', 'F'], ctrl: 'f' },
  settings: { group: 'anywhere', label: 'settings.open', caps: ['ctrl', ','], ctrl: ',', loose: true },
  running: { group: 'anywhere', label: 'run.toggle', caps: ['ctrl', '.'], ctrl: '.', loose: true },
  focus: { group: 'anywhere', label: 'keys.focus', caps: ['ctrl', 'B'], ctrl: 'b' },
  menu: { group: 'anywhere', label: 'keys.menu', caps: ['shift', 'f10'], also: ['menukey'] },
  sheet: { group: 'anywhere', label: 'keys.title', caps: ['?'] },
  close: { group: 'anywhere', label: 'keys.close', caps: ['esc'] },
  send: { group: 'message', label: 'keys.send', caps: ['enter'] },
  line: { group: 'message', label: 'keys.line', caps: ['shift', 'enter'] },
  earlier: { group: 'message', label: 'keys.earlier', caps: ['up'], also: ['down'] },
  pick: { group: 'message', label: 'keys.pick', caps: ['tab'], also: ['enter'] },
  search: { group: 'message', label: 'keys.search', caps: ['ctrl', 'R'], ctrl: 'r' },
} as const satisfies Record<string, Shortcut>;

export type ShortcutId = keyof typeof SHORTCUTS;

const CAPS: Partial<Record<Cap, Key>> = { ctrl: 'key.ctrl', shift: 'key.shift', enter: 'key.enter', esc: 'key.esc', menukey: 'key.menu' };
const SIGNS: Partial<Record<Cap, string>> = { up: '↑', down: '↓', tab: 'Tab', f10: 'F10' };

/** One cap as the person's keyboard prints it. */
export function cap(c: Cap): string {
  if (mac && c === 'ctrl') return '⌘';
  if (mac && c === 'shift') return '⇧';
  const word = CAPS[c];
  return word ? t(word) : (SIGNS[c] ?? c);
}

/** The caps of a shortcut as one line: "Ctrl N", "⌘N" on a Mac. */
export function keysOf(id: ShortcutId): string {
  const caps = SHORTCUTS[id].caps.map(cap);
  return mac ? caps.join('') : caps.join(' ');
}

/** Whether this key press is that Ctrl shortcut: Ctrl (or ⌘) and the key, with no Alt and no Shift unless it is loose. */
export function pressed(e: KeyboardEvent, id: ShortcutId): boolean {
  const s: Shortcut = SHORTCUTS[id];
  return !!s.ctrl && (e.ctrlKey || e.metaKey) && (!!s.loose || (!e.altKey && !e.shiftKey)) && e.key.toLowerCase() === s.ctrl;
}

/** The browser's own reload, caret browsing, print, save, view-source and find: Faber has no use for them, so they do
 * nothing and are not on the list (its own find is Ctrl+F above). */
export function browserKey(e: KeyboardEvent): boolean {
  const ctrl = e.ctrlKey || e.metaKey;
  return e.key === 'F5' || e.key === 'F7' || (ctrl && !e.altKey && ['r', 'p', 's', 'u', 'f'].includes(e.key.toLowerCase()));
}

/** Whether this key press is any Ctrl shortcut of the list: a layer that must be answered first holds them all back. */
export function pressedAny(e: KeyboardEvent): boolean {
  return (Object.keys(SHORTCUTS) as ShortcutId[]).some((id) => pressed(e, id));
}
