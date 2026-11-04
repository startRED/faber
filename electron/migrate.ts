/// <reference lib="dom" />
// Read a copy of Faber's old Chromium storage at its old origin; never open or rewrite the original profile.
import { WebContentsView, session } from 'electron';
import { copyFile, lstat, mkdir, readdir, rm } from 'node:fs/promises';
import path from 'node:path';

type PackedBlob = { data: string; type: string };
type Attachments = {
  images: { name?: string; mediaType: string; bytes: Blob }[];
  files: { name: string; bytes: Blob }[];
  inlined: { name: string; block: string }[];
};
type Draft = { text: string | null; attachments?: {
  images: { name?: string; mediaType: string; bytes: PackedBlob }[];
  files: { name: string; bytes: PackedBlob }[];
  inlined: Attachments['inlined'];
} };

/** Runs in a sandboxed blank page, without a preload, IPC, Node or network access. */
async function storage(mode: 'keys' | 'read' | 'write' | 'has' | 'done' | 'finished' | 'reconcile', key = '', value?: Draft): Promise<unknown> {
  const textKey = `faber.unsent.${key}`;
  const markerKey = `faber.unsent-attachments.${key}`;
  const done = 'faber.tauri-drafts-imported';
  const progress = `${done}.${key}`;
  if (mode === 'reconcile') {
    for (const name of Object.keys(localStorage)) {
      if (!name.startsWith(`${done}.`) || localStorage.getItem(name) !== 'pending') continue;
      const field = name.slice(done.length + 1);
      // Roll back before any legacy read can fail and let the screen hydrate a half-published field.
      localStorage.removeItem(`faber.unsent.${field}`);
      localStorage.removeItem(`faber.unsent-attachments.${field}`);
      localStorage.removeItem(name);
    }
    return;
  }
  if (mode === 'finished') return localStorage.getItem(done) === '1';
  if (mode === 'done') { localStorage.setItem(done, '1'); return; }
  if (mode === 'has') {
    const stage = localStorage.getItem(progress);
    if (stage === '1') return true;
    if (stage === 'pending') return false;
    if (localStorage.getItem(textKey) === null && localStorage.getItem(markerKey) === null) return false;
    localStorage.setItem(progress, '1');
    return true;
  }
  if (mode === 'keys') return [...new Set(Object.keys(localStorage).flatMap((name) => {
    if (name.startsWith('faber.unsent.')) return [name.slice('faber.unsent.'.length)];
    if (name.startsWith('faber.unsent-attachments.')) return [name.slice('faber.unsent-attachments.'.length)];
    return [];
  }))];
  async function record(value?: Attachments): Promise<Attachments | undefined> {
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const opening = indexedDB.open('faber.unsent', 1);
      opening.onupgradeneeded = () => opening.result.createObjectStore('attachments');
      opening.onerror = () => reject(opening.error);
      opening.onblocked = () => reject(new Error('Draft import blocked'));
      opening.onsuccess = () => resolve(opening.result);
    });
    try {
      return await new Promise((resolve, reject) => {
        const tx = db.transaction('attachments', value ? 'readwrite' : 'readonly', value ? { durability: 'strict' } : undefined);
        const store = tx.objectStore('attachments');
        const request = value ? store.put(value, key) : store.get(key);
        tx.oncomplete = () => resolve(value ?? request.result);
        tx.onabort = tx.onerror = () => reject(tx.error ?? new Error('Draft import interrupted'));
      });
    } finally { db.close(); }
  }
  if (mode === 'read') {
    const marker = localStorage.getItem(markerKey);
    if (marker === 'forget') return null;
    const draft: Draft = { text: localStorage.getItem(textKey) };
    if (marker === '1') {
      const saved = await record();
      if (!saved) throw new Error('Missing legacy attachment');
      const pack = async (blob: Blob): Promise<PackedBlob> => ({ type: blob.type, data: await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onerror = () => reject(reader.error);
        reader.onload = () => resolve(String(reader.result).split(',')[1]);
        reader.readAsDataURL(blob);
      }) });
      draft.attachments = {
        images: await Promise.all(saved.images.map(async (item) => ({ ...item, bytes: await pack(item.bytes) }))),
        files: await Promise.all(saved.files.map(async (item) => ({ ...item, bytes: await pack(item.bytes) }))),
        inlined: saved.inlined,
      };
    }
    return draft;
  }
  // A person may already have written or cleared this field in Electron. Never combine old files with newer words.
  if (!value || localStorage.getItem(progress) === '1') return;
  if (localStorage.getItem(progress) === 'pending') {
    const text = localStorage.getItem(textKey);
    const marker = localStorage.getItem(markerKey);
    if ((text !== null && text !== value.text) || (marker !== null && marker !== (value.attachments ? '1' : null))) {
      localStorage.setItem(progress, '1'); return;
    }
    // The previous process ended during publication, before the work screen could use this field.
    localStorage.removeItem(textKey); localStorage.removeItem(markerKey);
  } else if (localStorage.getItem(textKey) !== null || localStorage.getItem(markerKey) !== null) {
    localStorage.setItem(progress, '1'); return;
  }
  localStorage.setItem(progress, 'pending');
  try {
    if (value.attachments) {
      const unpack = (blob: PackedBlob) => new Blob([Uint8Array.from(atob(blob.data), (char) => char.charCodeAt(0))], { type: blob.type });
      await record({
        images: value.attachments.images.map((item) => ({ ...item, bytes: unpack(item.bytes) })),
        files: value.attachments.files.map((item) => ({ ...item, bytes: unpack(item.bytes) })),
        inlined: value.attachments.inlined,
      });
    }
    if (value.text !== null) localStorage.setItem(textKey, value.text);
    if (value.attachments) localStorage.setItem(markerKey, '1');
    localStorage.setItem(progress, '1');
  } catch (error) {
    localStorage.removeItem(textKey); localStorage.removeItem(markerKey); localStorage.removeItem(progress);
    throw error;
  }
}

function blank(): Response { return new Response('', { headers: { 'Content-Type': 'text/html', 'Content-Security-Policy': "default-src 'none'; script-src 'none'" } }); }
function page(ses: Electron.Session): WebContentsView {
  const view = new WebContentsView({ webPreferences: { session: ses, sandbox: true, contextIsolation: true, nodeIntegration: false, webviewTag: false, spellcheck: false } });
  view.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  view.webContents.on('will-navigate', (event) => event.preventDefault());
  return view;
}
const script = (mode: Parameters<typeof storage>[0], key?: string, value?: Draft) => `(${storage.toString()})(${JSON.stringify(mode)}, ${JSON.stringify(key ?? '')}, ${JSON.stringify(value ?? null)})`;

/** Windows' Tauri profile and Electron use Chromium; a clean shutdown already committed both storage formats. */
export async function migrateDrafts(dir: string): Promise<void> {
  if (process.platform !== 'win32') return;
  const data = await lstat(dir);
  if (!data.isDirectory() || data.isSymbolicLink()) throw new Error('Linked draft data directory');
  const writer = page(session.defaultSession);
  const snapshot = path.join(dir, 'draft-profile-import');
  let ownsSnapshot = false;
  try {
    const snapshotInfo = await lstat(snapshot).catch((error: NodeJS.ErrnoException) => { if (error.code !== 'ENOENT') throw error; return null; });
    if (snapshotInfo && (!snapshotInfo.isDirectory() || snapshotInfo.isSymbolicLink())) throw new Error('Linked draft snapshot');
    ownsSnapshot = true;
    await writer.webContents.loadURL('faber://app/__draft_import__');
    await writer.webContents.executeJavaScript(script('reconcile'));
    if (await writer.webContents.executeJavaScript(script('finished'))) return;
    const webview = path.join(dir, 'webview');
    const roots = [path.join(webview, 'EBWebView', 'Default'), path.join(webview, 'Default'), webview];
    for (let index = 0; index < roots.length; index++) {
      const root = roots[index];
      let exists = true;
      for (const relative of ['', ...path.relative(dir, root).split(path.sep).map((_name, i, parts) => path.join(...parts.slice(0, i + 1)))]) {
        const info = await lstat(path.join(dir, relative)).catch((error: NodeJS.ErrnoException) => { if (error.code !== 'ENOENT') throw error; return null; });
        if (!info) { exists = false; break; }
        if (!info.isDirectory() || info.isSymbolicLink()) throw new Error('Linked legacy profile');
      }
      if (!exists) continue;
      const copy = path.join(snapshot, String(index));
      await rm(copy, { recursive: true, force: true });
      await mkdir(copy, { recursive: true });
      async function copyStorage(source: string, target: string): Promise<void> {
        const info = await lstat(source);
        if (info.isSymbolicLink()) throw new Error('Linked legacy storage');
        if (info.isDirectory()) {
          await mkdir(target, { recursive: true });
          for (const name of await readdir(source)) {
            if (name !== 'LOCK') await copyStorage(path.join(source, name), path.join(target, name));
          }
        } else if (info.isFile()) await copyFile(source, target);
        else throw new Error('Unsupported legacy storage');
      }
      for (const name of ['Local Storage', 'IndexedDB']) {
        const source = path.join(root, name);
        const exists = await lstat(source).catch((error: NodeJS.ErrnoException) => { if (error.code !== 'ENOENT') throw error; return null; });
        if (exists) await copyStorage(source, path.join(copy, name));
      }
      const ses = session.fromPath(copy, { cache: false });
      ses.setPermissionRequestHandler((_wc, _permission, answer) => answer(false));
      ses.setPermissionCheckHandler(() => false);
      ses.protocol.handle('http', blank);
      ses.webRequest.onBeforeRequest((details, answer) => answer({ cancel: details.url !== 'http://tauri.localhost/' }));
      const reader = page(ses);
      try {
        await reader.webContents.loadURL('http://tauri.localhost/');
        const keys = await reader.webContents.executeJavaScript(script('keys')) as string[];
        for (const key of keys) {
          if (await writer.webContents.executeJavaScript(script('has', key))) continue;
          const draft = await reader.webContents.executeJavaScript(script('read', key)) as Draft | null;
          if (draft) await writer.webContents.executeJavaScript(script('write', key, draft));
        }
      } finally { reader.webContents.close(); }
    }
    await writer.webContents.executeJavaScript(script('done'));
    session.defaultSession.flushStorageData();
  } finally {
    session.defaultSession.flushStorageData();
    writer.webContents.close();
    // Chromium can hold the copy's database locks until exit; the next start removes it before reading again.
    if (ownsSnapshot) await rm(snapshot, { recursive: true, force: true }).catch(() => {});
  }
}
