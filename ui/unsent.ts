import { createEffect, createSignal, onCleanup } from 'solid-js';
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
// Conversation ids are never reused. Old callbacks cannot recreate a draft after its explicit deletion in this process.
const forgotten = new Set<string>();

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

/** Keeps `text` under `key`; an empty text (sent, cleared, cancelled) forgets it. Attachments are stored separately under the same key. A failed write
 * of text keeps the box reported as dirty until a write succeeds or the box is emptied. */
export function keep(key: string, text: string): boolean {
  if (forgotten.has(key) && text.trim()) return false;
  try {
    if (text.trim()) localStorage.setItem(KEPT + key, text);
    else localStorage.removeItem(KEPT + key);
    memory.delete(key);
    written(key, true);
    return true;
  } catch {
    memory.set(key, text);
    written(key, false);
    return false;
  }
}

/** The box that wrote under `key` is gone: its text is no longer on the screen, so a failed write stops counting. */
export function release(key: string): void {
  if (memory.get(key) === '') return; // A failed deletion must still block a restart that could restore private text.
  const owner = unwritten.get(key);
  if (owner) setDirty(owner, false);
  unwritten.delete(key);
}

/** Attachment bytes use IndexedDB; localStorage holds only a discoverable marker beside the existing text draft. */
export interface DraftAttachments {
  images: { name?: string; mediaType: string; base64: string }[];
  files: { name: string; base64: string }[];
  inlined: { name: string; block: string }[];
}
interface StoredAttachments {
  images: { name?: string; mediaType: string; bytes: Blob }[];
  files: { name: string; bytes: Blob }[];
  inlined: DraftAttachments['inlined'];
}
interface AttachmentChange { source?: object; text?: string; forget?: boolean; preserveReads?: boolean }
const ATTACHED = 'faber.unsent-attachments.';
const attachmentMemory = new Map<string, DraftAttachments>();
const attachmentWrites = new Map<string, Promise<boolean>>();
const attachmentAppends = new Map<string, Promise<boolean>>();
const attachmentOwners = new Map<string, object>();
const attachmentFailures = new Set<string>();
const attachmentForgets = new Set<string>();
const attachmentListeners = new Map<string, Set<(value: DraftAttachments, change: AttachmentChange) => void>>();
const attachmentReaders = new Map<string, number>();
const [attachmentStatus, setAttachmentStatus] = createSignal(0);
export function attachmentSaveFailed(key: string): boolean { attachmentStatus(); return attachmentFailures.has(key); }
const attachmentDiscards = new Map<string, number>();
export const attachmentDiscardVersion = (key: string): number => attachmentDiscards.get(key) ?? 0;
const attachmentVersions = new Map<string, number>();
const nonempty = (value: DraftAttachments): boolean => !!(value.images.length || value.files.length || value.inlined.length);
const emptyAttachments = (): DraftAttachments => ({ images: [], files: [], inlined: [] });

function releaseAttachmentCache(key: string): void {
  if (!attachmentListeners.has(key) && !attachmentReaders.has(key) && !attachmentOwners.has(key)) attachmentMemory.delete(key);
}

/** A live box sees a late file read from its disposed predecessor before a later keystroke can overwrite it. */
export function watchAttachments(key: string, listener: (value: DraftAttachments, change: AttachmentChange) => void): () => void {
  let listeners = attachmentListeners.get(key);
  if (!listeners) attachmentListeners.set(key, (listeners = new Set()));
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (!listeners.size) attachmentListeners.delete(key);
    releaseAttachmentCache(key);
  };
}

/** Retain the latest snapshot while selected files are being read, including after navigation. No DOM is retained. */
export function retainAttachments(key: string): () => void {
  attachmentReaders.set(key, (attachmentReaders.get(key) ?? 0) + 1);
  if (!hasKeptAttachments(key) && !attachmentMemory.has(key)) attachmentMemory.set(key, emptyAttachments());
  return () => {
    const count = (attachmentReaders.get(key) ?? 1) - 1;
    if (count) attachmentReaders.set(key, count);
    else attachmentReaders.delete(key);
    releaseAttachmentCache(key);
  };
}

export function hasKeptAttachments(key: string): boolean {
  const newer = attachmentMemory.get(key);
  if (newer) return nonempty(newer);
  try { return localStorage.getItem(ATTACHED + key) === '1'; }
  catch { return true; } // Unknown storage must not be mistaken for an empty saved draft.
}

/** A durable deletion intent hides stale bytes after a restart and lets the existing error toast retry their removal. */
export function pendingAttachmentRemovals(): string[] {
  const keys = new Set([...attachmentMemory].filter(([key, value]) => !nonempty(value) && attachmentOwners.has(key)).map(([key]) => key));
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key?.startsWith(ATTACHED) && ['remove', 'forget'].includes(localStorage.getItem(key) ?? '')) keys.add(key.slice(ATTACHED.length));
    }
  } catch { /* A failed marker write remains protected and discoverable in memory. */ }
  return [...keys];
}

function attachmentTransaction<T>(run: (store: IDBObjectStore) => IDBRequest<T>, mode: IDBTransactionMode): Promise<T> {
  return new Promise((resolve, reject) => {
    const opening = indexedDB.open('faber.unsent', 1);
    let abandoned = false;
    opening.onupgradeneeded = () => {
      if (!opening.result.objectStoreNames.contains('attachments')) opening.result.createObjectStore('attachments');
    };
    opening.onerror = () => reject(opening.error);
    opening.onblocked = () => { abandoned = true; reject(new Error('draft storage blocked')); };
    opening.onsuccess = () => {
      const db = opening.result;
      db.onversionchange = () => db.close();
      if (abandoned) { db.close(); return; }
      try {
        const tx = db.transaction('attachments', mode, mode === 'readwrite' ? { durability: 'strict' } : undefined);
        const request = run(tx.objectStore('attachments'));
        tx.oncomplete = () => { db.close(); resolve(request.result); };
        tx.onabort = tx.onerror = () => { db.close(); reject(tx.error ?? new Error('draft storage interrupted')); };
      } catch (error) { db.close(); reject(error); }
    };
  });
}

function attachmentProtection(key: string, on: boolean): void {
  let owner = attachmentOwners.get(key);
  if (on && !owner) attachmentOwners.set(key, (owner = {}));
  if (owner) setDirty(owner, on);
  if (!on) attachmentOwners.delete(key);
}
const bytesOf = (base64: string, type = '') => new Blob([Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))], { type });
const base64Of = (bytes: Blob): Promise<string> => new Promise((resolve, reject) => {
  const reader = new FileReader();
  reader.onload = () => resolve(String(reader.result).split(',')[1]!);
  reader.onerror = () => reject(reader.error);
  reader.readAsDataURL(bytes);
});

/** Protect pending/failed writes independently of the component. Markers track the latest intent before queued work. */
export function keepAttachments(key: string, value: DraftAttachments, change: AttachmentChange = {}): Promise<boolean> {
  if (forgotten.has(key) && nonempty(value)) return Promise.resolve(false);
  const removing = !nonempty(value);
  // Hydration can discard obsolete inline-text metadata without cancelling a new file the person is still selecting.
  if (removing && !change.preserveReads) attachmentDiscards.set(key, attachmentDiscardVersion(key) + 1);
  let markedRemoval = false;
  try { markedRemoval = ['remove', 'forget'].includes(localStorage.getItem(ATTACHED + key) ?? ''); } catch { /* Unknown storage stays protected below. */ }
  const hadAttachments = hasKeptAttachments(key) || attachmentOwners.has(key) || markedRemoval;
  const needed = nonempty(value) || hadAttachments || !!change.forget;
  if (change.forget) attachmentForgets.add(key);
  else attachmentForgets.delete(key);
  attachmentMemory.set(key, value);
  const version = (attachmentVersions.get(key) ?? 0) + 1;
  attachmentVersions.set(key, version);
  let markerError = false;
  if (needed) {
    attachmentProtection(key, true);
    try { localStorage.setItem(ATTACHED + key, change.forget ? 'forget' : removing ? 'remove' : '1'); }
    catch { markerError = true; }
  }
  const textRemoved = change.forget ? keep(key, '') : true;
  for (const listener of attachmentListeners.get(key) ?? []) listener(value, change);
  if (!needed) { releaseAttachmentCache(key); return Promise.resolve(true); }
  const save = (attachmentWrites.get(key) ?? Promise.resolve(true)).then(async () => {
    try {
      if (!removing) {
        if (markerError) throw new Error('draft marker could not be saved');
        const record: StoredAttachments = {
          images: value.images.map(({ name, mediaType, base64 }) => ({ name, mediaType, bytes: bytesOf(base64, mediaType) })),
          files: value.files.map(({ name, base64 }) => ({ name, bytes: bytesOf(base64) })),
          inlined: value.inlined,
        };
        await attachmentTransaction((store) => store.put(record, key), 'readwrite');
      } else {
        if (hadAttachments) await attachmentTransaction((store) => store.delete(key), 'readwrite');
        if (!textRemoved) throw new Error('draft text could not be removed');
        if (attachmentVersions.get(key) === version) localStorage.removeItem(ATTACHED + key);
      }
      if (attachmentVersions.get(key) === version) {
        attachmentProtection(key, false);
        attachmentForgets.delete(key);
        attachmentFailures.delete(key);
        releaseAttachmentCache(key);
        setAttachmentStatus((n) => n + 1);
      }
      return true;
    } catch {
      if (attachmentVersions.get(key) === version) { attachmentFailures.add(key); setAttachmentStatus((n) => n + 1); }
      return false;
    }
  });
  attachmentWrites.set(key, save);
  void save.then(() => { if (attachmentWrites.get(key) === save) attachmentWrites.delete(key); });
  return save;
}

/** Reads confirmed Blobs, or the latest live/pending snapshot. Durable bytes are released with the last box/file read. */
export async function keptAttachments(key: string): Promise<DraftAttachments | undefined> {
  const newer = attachmentMemory.get(key);
  if (newer) return nonempty(newer) ? newer : undefined;
  if (!hasKeptAttachments(key)) return;
  const version = attachmentVersions.get(key);
  const record = await attachmentTransaction<StoredAttachments | undefined>((store) => store.get(key), 'readonly');
  if (!record) {
    if (attachmentVersions.get(key) !== version) return keptAttachments(key);
    throw new Error('draft attachment record missing');
  }
  const value: DraftAttachments = {
    images: await Promise.all(record.images.map(async ({ name, mediaType, bytes }) => ({ name, mediaType, base64: await base64Of(bytes) }))),
    files: await Promise.all(record.files.map(async ({ name, bytes }) => ({ name, base64: await base64Of(bytes) }))),
    inlined: record.inlined,
  };
  if (attachmentVersions.get(key) !== version) return keptAttachments(key);
  if (attachmentListeners.has(key) || attachmentReaders.has(key)) attachmentMemory.set(key, value);
  return value;
}

/** Serialize late read-modify-writes so two selected files cannot both replace the same earlier snapshot. */
export function appendAttachments(key: string, added: DraftAttachments & { text: string }, discarded: number): Promise<boolean> {
  const append = (attachmentAppends.get(key) ?? Promise.resolve(true)).then(async () => {
    let saved: DraftAttachments | undefined;
    let version: number | undefined;
    do {
      if (attachmentDiscardVersion(key) !== discarded) return true;
      version = attachmentVersions.get(key);
      saved = await keptAttachments(key);
    } while (version !== attachmentVersions.get(key));
    if (attachmentDiscardVersion(key) !== discarded) return true;
    const text = kept(key);
    const gap = text ? '\n\n' : '';
    const combined = added.text ? text + gap + added.text : text;
    const inlined = added.inlined.map((item, i) => i === 0 ? { ...item, block: gap + item.block } : item);
    keep(key, combined);
    return keepAttachments(key, { images: [...(saved?.images ?? []), ...added.images], files: [...(saved?.files ?? []), ...added.files], inlined: [...(saved?.inlined ?? []), ...inlined] }, { text: combined });
  });
  attachmentAppends.set(key, append);
  void append.finally(() => { if (attachmentAppends.get(key) === append) attachmentAppends.delete(key); }).catch(() => {});
  return append;
}

/** Retry the exact saved intent: clearing attachments after Send must preserve any newer typed text. */
export async function retryDraftRemovals(): Promise<boolean> {
  const results = await Promise.all(pendingAttachmentRemovals().map((key) => {
    let forget = attachmentForgets.has(key);
    try { forget ||= localStorage.getItem(ATTACHED + key) === 'forget'; } catch { /* The in-memory scope remains. */ }
    return forget ? forgetDraft(key) : keepAttachments(key, emptyAttachments());
  }));
  return results.every(Boolean);
}

/** An explicit conversation removal clears both forms, with its own durable scope before either deletion. */
export function forgetDraft(key: string): Promise<boolean> {
  forgotten.add(key);
  return keepAttachments(key, emptyAttachments(), { text: '', forget: true });
}
