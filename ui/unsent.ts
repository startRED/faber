import { createEffect, onCleanup } from 'solid-js';
import { api } from './api.ts';

/** The fields that hold something the person has typed or attached and not sent yet. */
const dirty = new Set<object>();
let told = false;

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

export function kept(key: string): string {
  try {
    return localStorage.getItem(KEPT + key) ?? '';
  } catch {
    return '';
  }
}

/** Keeps `text` under `key`; an empty text (sent, cleared, cancelled) forgets it. Attachments are not kept. */
export function keep(key: string, text: string): void {
  try {
    if (text.trim()) localStorage.setItem(KEPT + key, text);
    else localStorage.removeItem(KEPT + key);
  } catch {
    // Storage can be unavailable; the draft just lives on the screen.
  }
}
