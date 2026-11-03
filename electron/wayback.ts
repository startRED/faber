import { closeSync, constants, openSync, writeSync, lstatSync, mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { lstat, mkdir, open, readdir, realpath, rm, statfs, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import path from 'node:path';

const marker = 'backup-before-electron.done';
const counter = 'electron-starts';
const maxFiles = 10_000;
const maxBytes = 2 * 1024 * 1024 * 1024;

function log(dir: string, message: string): void {
  try {
    const file = path.join(dir, 'update.log');
    try { if (lstatSync(file).isSymbolicLink()) return; } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') return; }
    const fd = openSync(file, constants.O_APPEND | constants.O_CREAT | constants.O_WRONLY | (constants.O_NOFOLLOW ?? 0));
    try { writeSync(fd, `${new Date().toISOString()} ${message}\n`); } finally { closeSync(fd); }
  } catch { /* Startup must continue. */ }
}

export function enoughSpace(available: number, size: number): boolean { return available >= size * 2; }

async function backup(dir: string): Promise<void> {
  let dest: string | undefined;
  let created = false;
  try {
    const root = await realpath(dir);
    try { await lstat(path.join(root, marker)); return; } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
    let count = 0;
    let bytes = 0;
    const files: string[] = [];
    async function checked(relative: string) {
      const full = path.join(root, relative);
      const parts = relative.split(path.sep);
      for (let i = 1; i <= parts.length; i++) {
        const part = path.join(root, ...parts.slice(0, i));
        if ((await lstat(part)).isSymbolicLink()) throw new Error('Linked user data');
        const actual = await realpath(part);
        if (actual !== root && !actual.startsWith(root + path.sep)) throw new Error('User data outside data directory');
      }
      return lstat(full);
    }
    async function visit(relative: string): Promise<void> {
      const info = await checked(relative);
      if (info.isDirectory()) {
        for (const name of await readdir(path.join(root, relative))) await visit(path.join(relative, name));
      } else if (info.isFile()) {
        count++;
        bytes += info.size;
        if (count > maxFiles || bytes > maxBytes) throw new RangeError('Backup skipped: file count or size limit exceeded.');
        files.push(relative);
      } else throw new Error('Unsupported user data');
    }
    for (const entry of await readdir(root, { withFileTypes: true })) {
      if (entry.name === 'conversations' || entry.name.endsWith('.json')) await visit(entry.name);
    }
    const space = await statfs(root);
    if (!enoughSpace(space.bavail * space.bsize, bytes)) { log(dir, 'Backup skipped: not enough free disk space.'); return; }
    dest = path.join(root, `backup-before-electron-${randomUUID()}`);
    await mkdir(dest);
    created = true;
    let copiedBytes = 0;
    for (const file of files) {
      const info = await checked(file);
      if (!info.isFile()) throw new Error('User data changed');
      const source = await open(path.join(root, file), constants.O_RDONLY | (constants.O_NOFOLLOW || 0));
      try {
        const opened = await source.stat();
        const after = await checked(file);
        if (!opened.isFile() || opened.dev !== after.dev || opened.ino !== after.ino) throw new Error('User data changed');
        copiedBytes += opened.size;
        if (copiedBytes > maxBytes) throw new RangeError('Backup skipped: file count or size limit exceeded.');
        const target = path.join(dest, file);
        await mkdir(path.dirname(target), { recursive: true });
        const output = await open(target, 'wx');
        try {
          const buffer = Buffer.alloc(64 * 1024);
          let position = 0;
          while (position < opened.size) {
            const { bytesRead } = await source.read(buffer, 0, Math.min(buffer.length, opened.size - position), position);
            if (!bytesRead) throw new Error('User data changed');
            await output.writeFile(buffer.subarray(0, bytesRead));
            position += bytesRead;
          }
        } finally { await output.close(); }
      } finally { await source.close(); }
    }
    await writeFile(path.join(root, marker), `${path.basename(dest)}\n`, { flag: 'wx' });
    dest = undefined;
  } catch (error) {
    if (created && dest && path.dirname(dest) === await realpath(dir).catch(() => '') && path.basename(dest).startsWith('backup-before-electron-')) {
      await rm(dest, { recursive: true, force: true }).catch(() => log(dir, 'Backup cleanup failed.'));
    }
    log(dir, error instanceof RangeError ? error.message : 'Backup failed; it will be retried.');
  }
}

/** Count synchronously, but never delay window creation for the backup. */
export function startWayback(dir: string): Promise<void> {
  try {
    mkdirSync(dir, { recursive: true });
    const target = path.join(dir, counter);
    let previous = 0;
    try {
      if (lstatSync(target).isSymbolicLink()) throw new Error('Linked startup guard');
      previous = Number(readFileSync(target, 'utf8')) || 0;
    } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
    if (previous >= 2) log(dir, 'Electron failed to reach ready twice. See Going back to the Tauri build in the architecture documentation.');
    const temp = path.join(dir, `.electron-starts-${randomUUID()}`);
    try {
      writeFileSync(temp, String(Math.min(previous + 1, 2)), { flag: 'wx' });
      renameSync(temp, target);
    } finally { try { unlinkSync(temp); } catch { /* Renamed or not created. */ } }
  } catch { log(dir, 'Startup guard failed.'); }
  return backup(dir);
}

export function readyWayback(dir: string): void {
  try { unlinkSync(path.join(dir, counter)); } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') log(dir, 'Startup guard could not clear.');
  }
}
