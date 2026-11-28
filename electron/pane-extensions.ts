import { createHash, randomUUID } from 'node:crypto';
import { constants } from 'node:fs';
import { lstat, mkdir, open, readdir, realpath, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import type { Session } from 'electron';
import { plainProfile } from './panerules.ts';

const invalidFolder = () => new Error('an unpacked extension must be a canonical local folder with regular, unlinked files');
const maxBytes = 64 * 1024 * 1024;

function localPath(folder: unknown): folder is string {
  if (typeof folder !== 'string' || !path.isAbsolute(folder) || path.normalize(folder) !== folder) return false;
  if (process.platform !== 'win32') return true;
  return /^[a-z]:\\/i.test(folder) && !folder.slice(2).includes(':') &&
    folder.slice(3).split('\\').every((part) => !/[. ]$|^(?:con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³])(?:\.|$)/i.test(part));
}

type Description = { name: string; version: string; folder: string; failed?: boolean };

function within(root: string, folder: string): boolean {
  const relative = path.relative(root, folder);
  return relative !== '..' && !relative.startsWith('..' + path.sep) && !path.isAbsolute(relative);
}

async function inspect(folder: unknown, projectRoot?: string, snapshot?: string, blocked: readonly string[] = []): Promise<Description> {
  if (!localPath(folder)) throw invalidFolder();
  const protectedPaths = new Set(blocked);
  for (const root of blocked) {
    const canonical = await realpath(root).catch(() => undefined);
    if (canonical) protectedPaths.add(canonical);
  }
  const rejectProtected = (name: string) => {
    if ([...protectedPaths].some((root) => within(root, name) || within(name, root))) throw invalidFolder();
  };
  rejectProtected(folder);
  const canonical = await realpath(folder);
  rejectProtected(canonical);
  if (canonical !== folder) throw invalidFolder();
  if (projectRoot !== undefined) {
    const project = await realpath(projectRoot);
    if (folder === project || !within(project, folder)) throw invalidFolder();
  }
  let count = 0;
  let bytes = 0;
  let manifest: { name?: unknown; version?: unknown; manifest_version?: unknown } | undefined;
  async function walk(source: string, destination?: string, depth = 0): Promise<void> {
    if (++count > 4096 || depth > 32) throw invalidFolder();
    const stat = await lstat(source, { bigint: true });
    if (stat.isSymbolicLink() || (await realpath(source)) !== source) throw invalidFolder();
    if (stat.isDirectory()) {
      if (destination) await mkdir(destination);
      for (const part of await readdir(source)) {
        const child = path.join(source, part);
        if (!localPath(child)) throw invalidFolder();
        await walk(child, destination && path.join(destination, part), depth + 1);
      }
      return;
    }
    if (depth === 0 || !stat.isFile() || stat.nlink !== 1n) throw invalidFolder();
    bytes += Number(stat.size);
    if (bytes > maxBytes) throw invalidFolder();
    const isManifest = depth === 1 && path.basename(source) === 'manifest.json';
    if (!destination && !isManifest) return;
    if (isManifest && stat.size > 1024n * 1024n) throw invalidFolder();
    const file = await open(source, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0) | (constants.O_NONBLOCK ?? 0));
    try {
      const opened = await file.stat({ bigint: true });
      if (!opened.isFile() || opened.nlink !== 1n || opened.dev !== stat.dev || opened.ino !== stat.ino ||
          opened.size !== stat.size || (await realpath(source)) !== source) throw invalidFolder();
      const buffer = Buffer.alloc(Number(stat.size));
      if ((await file.read(buffer, 0, buffer.length, 0)).bytesRead !== buffer.length) throw invalidFolder();
      const after = await file.stat({ bigint: true });
      if (after.nlink !== 1n || after.mtimeNs !== opened.mtimeNs || after.size !== opened.size) throw invalidFolder();
      if (isManifest) manifest = JSON.parse(buffer.toString('utf8'));
      if (destination) await writeFile(destination, buffer, { flag: 'wx' });
    } finally {
      await file.close();
    }
  }
  await walk(folder, snapshot);
  if (typeof manifest?.name !== 'string' || !manifest.name || typeof manifest.version !== 'string' || !manifest.version ||
      (manifest.manifest_version !== 2 && manifest.manifest_version !== 3)) throw invalidFolder();
  return { folder, name: manifest.name, version: manifest.version };
}

export async function resolveExtensionFolder(source: string, projectRoot?: string, blocked?: readonly string[]): Promise<string> {
  return (await inspect(source, projectRoot, undefined, blocked)).folder;
}

// Only this dedicated root is removed. An alias at the root is refused rather than followed.
export async function resetExtensionStorage(root: string): Promise<string> {
  const parent = await realpath(path.dirname(root));
  const storage = path.join(parent, path.basename(root));
  const previous = await lstat(storage).catch((error: NodeJS.ErrnoException) => {
    if (error.code !== 'ENOENT') throw error;
  });
  if (previous) {
    if (!previous.isDirectory() || previous.isSymbolicLink() || (await realpath(storage)) !== storage) throw invalidFolder();
    await rm(storage, { recursive: true });
  }
  await mkdir(storage, { mode: 0o700 });
  return storage;
}

type Slot = { directory: string; session: Session; scope?: string; generation?: number; clean: boolean };
const hash = (value: string) => createHash('sha256').update(value).digest('hex');
const safeScope = (scope: unknown): scope is string => plainProfile(scope) && scope !== '.' && scope !== '..';

export class PaneExtensions {
  private enabled = false;
  private generation = 0;
  private tail: Promise<unknown> = Promise.resolve();
  private registry = new Map<string, Map<string, Description>>();
  private registryLoaded = false;
  private readonly slots: Slot[] = [];

  constructor(
    privateRoot: () => string | undefined,
    makeSession: (folder: string, allow: (url: string) => boolean) => Session,
    wipe: (session: Session) => Promise<void>,
    registryFile?: string,
    blocked: readonly string[] = [],
  ) {
    this.privateRoot = privateRoot;
    this.makeSession = makeSession;
    this.wipe = wipe;
    this.registryFile = registryFile;
    this.blocked = blocked;
  }

  private readonly privateRoot: () => string | undefined;
  private readonly makeSession: (folder: string, allow: (url: string) => boolean) => Session;
  private readonly wipe: (session: Session) => Promise<void>;
  private readonly registryFile?: string;
  private readonly blocked: readonly string[];

  private async loadRegistry(): Promise<void> {
    if (this.registryLoaded) return;
    const source = this.registryFile;
    if (source) {
      const stat = await lstat(source, { bigint: true }).catch((error: NodeJS.ErrnoException) => {
        if (error.code !== 'ENOENT') throw error;
      });
      if (stat) {
        if (!stat.isFile() || stat.nlink !== 1n || stat.size > 4n * 1024n * 1024n) throw new Error('invalid extension registry');
        const file = await open(source, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0) | (constants.O_NONBLOCK ?? 0));
        try {
          const opened = await file.stat({ bigint: true });
          if (!opened.isFile() || opened.nlink !== 1n || opened.dev !== stat.dev || opened.ino !== stat.ino ||
              opened.size !== stat.size) throw new Error('invalid extension registry');
          const buffer = Buffer.alloc(Number(stat.size));
          if ((await file.read(buffer, 0, buffer.length, 0)).bytesRead !== buffer.length) throw new Error('invalid extension registry');
          const rows = JSON.parse(buffer.toString('utf8')) as [string, Description[]][];
          if (!Array.isArray(rows) || rows.length > 64) throw new Error('invalid extension registry');
          const registry = new Map<string, Map<string, Description>>();
          for (const [scope, entries] of rows) {
            if (!safeScope(scope) || !Array.isArray(entries) || entries.length > 16) throw new Error('invalid extension registry');
            const folders = new Map<string, Description>();
            for (const { folder, name, version } of entries) {
              if (!localPath(folder) || typeof name !== 'string' || typeof version !== 'string') throw new Error('invalid extension registry');
              folders.set(folder, { folder, name, version });
            }
            registry.set(scope, folders);
          }
          this.registry = registry;
        } finally {
          await file.close();
        }
      }
    }
    this.registryLoaded = true;
  }

  private async saveRegistry(registry: Map<string, Map<string, Description>>): Promise<void> {
    if (this.registryFile) {
      const temporary = this.registryFile + '.' + randomUUID() + '.tmp';
      const rows = [...registry].map(([scope, entries]) => [
        scope, [...entries.values()].map(({ folder, name, version }) => ({ folder, name, version })),
      ]);
      const content = JSON.stringify(rows);
      if (Buffer.byteLength(content) > 4 * 1024 * 1024) throw new Error('extension registry is full');
      try {
        await writeFile(temporary, content, { flag: 'wx', mode: 0o600 });
        await rename(temporary, this.registryFile);
      } finally {
        await rm(temporary, { force: true });
      }
    }
    this.registry = registry;
  }

  private enqueue<T>(work: () => Promise<T>): Promise<T> {
    const pending = this.tail.then(work);
    this.tail = pending.catch(() => {});
    return pending;
  }

  mode(enabled: boolean): Promise<void> {
    if (this.enabled !== enabled) ++this.generation;
    this.enabled = enabled;
    if (enabled) return this.enqueue(() => this.loadRegistry());
    return this.enqueue(async () => {
      const cleaned = await Promise.allSettled(this.slots.filter((slot) => !slot.clean).map((slot) => this.clean(slot)));
      if (cleaned.some((result) => result.status === 'rejected')) throw new Error('extension profile cleanup failed');
    });
  }

  scope(value: unknown): string {
    if (!this.enabled) throw new Error('extension developer mode is off');
    if (!safeScope(value)) throw new Error('invalid extension scope');
    return value;
  }

  forget(scope: unknown): Promise<void> {
    if (!safeScope(scope)) throw new Error('invalid extension scope');
    return this.enqueue(async () => {
      await this.loadRegistry();
      if (!this.registry.has(scope)) return;
      const registry = new Map(this.registry);
      registry.delete(scope);
      await this.saveRegistry(registry);
    });
  }

  acquire(scope: string): Promise<Session> {
    const generation = this.generation;
    return this.enqueue(async () => {
      this.scope(scope);
      if (generation !== this.generation) throw new Error('extension mode changed');
      await this.loadRegistry();
      const root = this.privateRoot();
      if (!root) throw new Error('extension profile storage is unavailable');
      let slot = this.slots.find((entry) => entry.clean);
      if (!slot) {
        if (this.slots.length >= 4) throw new Error('no clean extension profiles are available');
        const directory = path.join(root, String(this.slots.length));
        await mkdir(directory);
        const permits = (url: string) => {
          const current = this.slots.find((entry) => entry.directory === directory);
          if (!this.enabled || !current?.scope || current.generation !== this.generation) return false;
          if (!url.startsWith('chrome-extension:')) return true;
          const id = /^chrome-extension:\/\/([a-p]{32})(?:\/|$)/.exec(url)?.[1];
          return !!id && !!current.session.extensions.getExtension(id);
        };
        slot = { directory, session: this.makeSession(path.join(directory, 'profile'), permits), clean: true };
        this.slots.push(slot);
      }
      slot.scope = scope;
      slot.generation = generation;
      slot.clean = false;
      for (const extension of this.registry.get(scope)?.values() ?? []) await this.install(slot, extension);
      if (!this.enabled || generation !== this.generation) throw new Error('extension mode changed');
      return slot.session;
    });
  }

  release(scope: string): Promise<void> {
    return this.enqueue(async () => {
      const slot = this.slots.find((entry) => entry.scope === scope);
      if (slot) await this.clean(slot);
    });
  }

  clear(scope: string): Promise<void> {
    return this.enqueue(async () => {
      const slot = this.slots.find((entry) => entry.scope === scope);
      if (slot) await this.wipe(slot.session);
    });
  }

  private async clean(slot: Slot): Promise<void> {
    slot.scope = undefined;
    let unloaded = true;
    for (const extension of slot.session.extensions.getAllExtensions()) {
      try { slot.session.extensions.removeExtension(extension.id); } catch { unloaded = false; }
    }
    const results = await Promise.allSettled([
      this.wipe(slot.session), rm(path.join(slot.directory, 'extensions'), { recursive: true, force: true }),
    ]);
    if (!unloaded || results.some((result) => result.status === 'rejected')) throw new Error('extension profile cleanup failed');
    slot.clean = true;
  }

  private snapshot(slot: Slot, folder: string): string {
    return path.join(slot.directory, 'extensions', hash(folder));
  }

  private async uninstall(slot: Slot, folder: string): Promise<void> {
    const snapshot = this.snapshot(slot, folder);
    for (const extension of slot.session.extensions.getAllExtensions()) {
      if (extension.path === snapshot) slot.session.extensions.removeExtension(extension.id);
    }
    await rm(snapshot, { recursive: true, force: true });
  }

  private async install(slot: Slot, extension: Description): Promise<void> {
    const snapshot = this.snapshot(slot, extension.folder);
    try {
      await mkdir(path.dirname(snapshot), { recursive: true, mode: 0o700 });
      const validated = await inspect(extension.folder, undefined, snapshot, this.blocked);
      if (!this.enabled || slot.generation !== this.generation) throw new Error('extension mode changed');
      const loaded = await slot.session.extensions.loadExtension(snapshot, { allowFileAccess: false });
      if (!this.enabled || slot.generation !== this.generation) throw new Error('extension mode changed');
      Object.assign(extension, validated, { name: loaded.name, version: loaded.version });
      delete extension.failed;
    } catch {
      extension.failed = true;
      await this.uninstall(slot, extension.folder);
    }
  }

  act(params: unknown): Promise<Description[]> {
    const input = params as { scope?: unknown; action?: unknown; folder?: unknown } | null;
    const scope = this.scope(input?.scope);
    const action = input?.action;
    const folder = input?.folder;
    const generation = this.generation;
    return this.enqueue(async () => {
      if (!this.enabled || generation !== this.generation) throw new Error('extension mode changed');
      await this.loadRegistry();
      const entries = new Map(this.registry.get(scope));
      const slot = this.slots.find((entry) => entry.scope === scope);
      if (action === 'list') return [...entries.values()].map((entry) => ({ ...entry }));
      if ((action !== 'load' && action !== 'reload' && action !== 'remove') || !localPath(folder)) throw new Error('invalid extension operation');
      const registry = new Map(this.registry);
      if (action === 'remove') {
        entries.delete(folder);
        if (entries.size) registry.set(scope, entries);
        else registry.delete(scope);
        await this.saveRegistry(registry);
        if (slot) await this.uninstall(slot, folder);
      } else {
        const previous = entries.get(folder);
        if (action === 'reload' && !previous) throw new Error('extension is not registered');
        if (!previous && (entries.size >= 16 || (!this.registry.has(scope) && this.registry.size >= 64))) throw new Error('extension registry is full');
        const extension = await inspect(folder, undefined, undefined, this.blocked);
        if (!this.enabled || generation !== this.generation) throw new Error('extension mode changed');
        entries.set(folder, extension);
        registry.set(scope, entries);
        await this.saveRegistry(registry);
        if (slot) {
          await this.uninstall(slot, folder);
          await this.install(slot, extension);
        }
      }
      return [...entries.values()].map((entry) => ({ ...entry }));
    });
  }
}
