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
  group: 'anywhere' | 'message' | 'browser';
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
  /** Shift is part of the key (Ctrl+Shift+D), and the same key without it is another one or none. */
  shift?: true;
}

export const SHORTCUTS = {
  new: { group: 'anywhere', label: 'side.new', caps: ['ctrl', 'N'], ctrl: 'n', loose: true },
  palette: { group: 'anywhere', label: 'palette.label', caps: ['ctrl', 'K'], ctrl: 'k' },
  find: { group: 'anywhere', label: 'keys.find', caps: ['ctrl', 'F'], ctrl: 'f' },
  settings: { group: 'anywhere', label: 'settings.open', caps: ['ctrl', ','], ctrl: ',', loose: true },
  running: { group: 'anywhere', label: 'run.toggle', caps: ['ctrl', '.'], ctrl: '.', loose: true },
  // Loose: a semicolon takes Shift on German and Spanish keyboards.
  aside: { group: 'anywhere', label: 'aside.title', caps: ['ctrl', ';'], ctrl: ';', loose: true },
  // The topbar's other two pane buttons, with Shift: Ctrl+D and Ctrl+B alone are taken or too easy to hit.
  changes: { group: 'anywhere', label: 'changes.title', caps: ['ctrl', 'shift', 'D'], ctrl: 'd', shift: true },
  browser: { group: 'anywhere', label: 'site.browser', caps: ['ctrl', 'shift', 'B'], ctrl: 'b', shift: true },
  focus: { group: 'anywhere', label: 'keys.focus', caps: ['ctrl', 'B'], ctrl: 'b' },
  menu: { group: 'anywhere', label: 'keys.menu', caps: ['shift', 'f10'], also: ['menukey'] },
  sheet: { group: 'anywhere', label: 'keys.title', caps: ['?'] },
  close: { group: 'anywhere', label: 'keys.close', caps: ['esc'] },
  send: { group: 'message', label: 'keys.send', caps: ['enter'] },
  line: { group: 'message', label: 'keys.line', caps: ['shift', 'enter'] },
  earlier: { group: 'message', label: 'keys.earlier', caps: ['up'], also: ['down'] },
  pick: { group: 'message', label: 'keys.pick', caps: ['tab'], also: ['enter'] },
  search: { group: 'message', label: 'keys.search', caps: ['ctrl', 'R'], ctrl: 'r' },
  // Faber's browser answers these itself while it has the focus (`siteKeyOf` on the screen, the shell's `site_key` on the
  // page, which is a native view the screen never hears keys from), so they carry no `ctrl`. Alt is printed Alt everywhere.
  siteNew: { group: 'browser', label: 'keys.siteNew', caps: ['ctrl', 'T'] },
  siteReopen: { group: 'browser', label: 'keys.siteReopen', caps: ['ctrl', 'shift', 'T'] },
  siteClose: { group: 'browser', label: 'keys.siteClose', caps: ['ctrl', 'W'] },
  siteNext: { group: 'browser', label: 'keys.siteNext', caps: ['ctrl', 'tab'] },
  sitePrevious: { group: 'browser', label: 'keys.sitePrevious', caps: ['ctrl', 'shift', 'tab'] },
  siteTab: { group: 'browser', label: 'keys.siteTab', caps: ['ctrl', '1–9'] },
  siteAddress: { group: 'browser', label: 'keys.siteAddress', caps: ['ctrl', 'L'] },
  siteReload: { group: 'browser', label: 'site.reload', caps: ['ctrl', 'R'] },
  siteBack: { group: 'browser', label: 'site.back', caps: ['Alt', '←'] },
  siteForward: { group: 'browser', label: 'site.forward', caps: ['Alt', '→'] },
} as const satisfies Record<string, Shortcut>;

/** What a key the browser answers does, by the name the shell's `site_key` sends for the same key on the page. */
export type SiteKey = 'new' | 'reopen' | 'close' | 'next' | 'previous' | 'address' | 'reload' | 'back' | 'forward' | `tab${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9}`;

/** The browser's key this press is, while the focus is in the pane's own head and bar. Ctrl and a digit are the sidebar's
 * there: they pick tabs only from the page. AltGr (Ctrl and Alt) types a character on many keyboards and is never one. */
export function siteKeyOf(e: KeyboardEvent): SiteKey | undefined {
  const ctrl = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  if (ctrl && !e.altKey) {
    if (key === 't') return e.shiftKey ? 'reopen' : 'new';
    if (key === 'tab') return e.shiftKey ? 'previous' : 'next';
    if (e.shiftKey) return undefined;
    return ({ w: 'close', l: 'address', r: 'reload' } as const)[key as 'w' | 'l' | 'r'];
  }
  if (e.altKey && !ctrl && !e.shiftKey) return key === 'arrowleft' ? 'back' : key === 'arrowright' ? 'forward' : undefined;
  return undefined;
}

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

/** Whether this key press is that Ctrl shortcut: Ctrl (or ⌘) and the key, with no Alt, and Shift only where the key takes it
 * (or anything where it is loose). */
export function pressed(e: KeyboardEvent, id: ShortcutId): boolean {
  const s: Shortcut = SHORTCUTS[id];
  return !!s.ctrl && (e.ctrlKey || e.metaKey) && (!!s.loose || (!e.altKey && e.shiftKey === !!s.shift)) && e.key.toLowerCase() === s.ctrl;
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
