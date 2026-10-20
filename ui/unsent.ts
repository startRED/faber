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

// The text a failed write could not save, by the same keys: a box that is built again (scrolling that loads earlier messages)
// reads this before the storage, so stale text never comes back over newer text. Gone on the next successful write.
const memory = new Map<string, string>();

export function kept(key: string): string {
  const newer = memory.get(key);
  if (newer !== undefined) return newer;
  try {
    return localStorage.getItem(KEPT + key) ?? '';
  } catch {
    return '';
  }
}

/** Saved queue editors can outlive their rows or the whole screen. Read them without an editor registry. */
export function queuedDrafts(conversationId: string): string[] {
  const prefix = `queued-edit.${conversationId}.`;
  const keys = new Set([...memory.keys()].filter((key) => key.startsWith(prefix)));
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key?.startsWith(KEPT + prefix)) keys.add(key.slice(KEPT.length));
    }
  } catch { /* Failed writes remain readable in memory. */ }
  return [...keys].map((key) => key.slice(prefix.length));
}

/** Returns a lost edit to its conversation's message box, preserving any words already there. */
export function recoverQueuedDraft(conversationId: string, turnId: string): string | undefined {
  const key = `queued-edit.${conversationId}.${turnId}`;
  const edit = kept(key);
  if (!edit.trim()) return;
  const existing = kept(conversationId);
  const text = existing ? `${existing}\n\n${edit}` : edit;
  keep(conversationId, text);
  keep(key, '');
  return text;
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

/** The box that wrote under `key` is gone: its text is no longer on the screen, so a failed write stops counting. */
export function release(key: string): void {
  const owner = unwritten.get(key);
  if (owner) setDirty(owner, false);
  unwritten.delete(key);
}
