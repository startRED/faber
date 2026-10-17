import { createEffect, onCleanup } from 'solid-js';
import { api } from './api.ts';

/** The fields that hold something the person has typed or attached and not sent yet. */
const dirty = new Set<object>();
let told = false;

/** How many fields hold something unsent (or failed to write it). */
export const dirtyCount = (): number => dirty.size;

/** Tells the shell when the first such field appears and when the last one goes: an update never closes the window on a draft. */
export function setDirty(owner: object, on: boolean): void {
  if (on) dirty.add(owner);
  else dirty.delete(owner);
  const now = dirty.size > 0;
  if (now === told) return;
  told = now;
  void api.draftDirty(now).catch(() => {});
}

/** Reports `unsent()` for the component that calls it; gone with the component. */
export function useDraft(unsent: () => boolean): void {
  const owner = {};
  createEffect(() => setDirty(owner, unsent()));
  onCleanup(() => setDirty(owner, false));
}

/** What was typed and not sent, kept per field in the webview's storage, so a restart for an update (or a crash) loses nothing. */
const KEPT = 'faber.unsent.';

// Text typed a moment ago and not written yet, by the same keys: a write per key press would cost a storage write each, so a
// pause, a blur, the end of the page or the box going away writes it. `kept` reads it first, so nothing is ever older than what was typed.
const SOON_MS = 400;
const soon = new Map<string, { text: string; timer: ReturnType<typeof setTimeout> }>();

// The text a failed write could not save, by the same keys: a box that is built again (scrolling that loads earlier messages)
// reads this before the storage, so stale text never comes back over newer text. Gone on the next successful write.
const memory = new Map<string, string>();

export function kept(key: string): string {
  const typed = soon.get(key);
  if (typed) return typed.text;
  const newer = memory.get(key);
  if (newer !== undefined) return newer;
  try {
    return localStorage.getItem(KEPT + key) ?? '';
  } catch {
    return '';
  }
}

// A box whose text could not be written stays reported as dirty, so no unattended install ends the window on text that
// lives only in memory; the next successful write, or clearing the box, lifts it.
const unwritten = new Map<string, object>();
function written(key: string, ok: boolean): void {
  let owner = unwritten.get(key);
  if (!ok && !owner) unwritten.set(key, (owner = {}));
  if (owner) setDirty(owner, !ok);
  if (ok) unwritten.delete(key);
}

/** Keeps `text` under `key`; an empty text (sent, cleared, cancelled) forgets it. Attachments are not kept. A failed write
 * of text keeps the box reported as dirty until a write succeeds or the box is emptied. */
export function keep(key: string, text: string): void {
  const typed = soon.get(key);
  if (typed) {
    clearTimeout(typed.timer);
    soon.delete(key);
  }
  try {
    if (text.trim()) localStorage.setItem(KEPT + key, text);
    else localStorage.removeItem(KEPT + key);
    memory.delete(key);
    written(key, true);
  } catch {
    if (text.trim()) memory.set(key, text);
    else memory.delete(key);
    written(key, !text.trim());
  }
}

/** `keep` for text that is still being typed: written when the typing pauses, or at once by `flushKept`. An emptied box (sent,
 * cleared, cancelled) forgets its text at once. */
export function keepSoon(key: string, text: string): void {
  const typed = soon.get(key);
  if (typed) clearTimeout(typed.timer);
  if (!text.trim()) return keep(key, text);
  soon.set(key, { text, timer: setTimeout(() => flushKept(key), SOON_MS) });
}

/** Writes what `keepSoon` is holding: for one key (the box lost focus or is going away), or for all (the window hides or closes). */
export function flushKept(key?: string): void {
  for (const k of key === undefined ? [...soon.keys()] : [key]) {
    const typed = soon.get(k);
    if (typed) keep(k, typed.text);
  }
}

if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => flushKept());
  window.addEventListener('blur', () => flushKept());
  document.addEventListener('visibilitychange', () => document.hidden && flushKept());
}

/** The box that wrote under `key` is gone: its text is no longer on the screen, so a failed write stops counting. */
export function release(key: string): void {
  flushKept(key);
  const owner = unwritten.get(key);
  if (owner) setDirty(owner, false);
  unwritten.delete(key);
}
