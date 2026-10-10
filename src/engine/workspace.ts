import { existsSync, mkdirSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { basename, extname, join, relative, resolve, sep } from 'node:path';

// Writes attached files into `dir` under safe, unique names and returns the full paths.
export function saveFiles(dir: string, files: { name: string; base64: string }[]): string[] {
  mkdirSync(dir, { recursive: true });
  return files.map((file) => {
    const name = basename(file.name.replace(/\\/g, '/'));
    const clean = [...name].map((ch) => (ch.charCodeAt(0) < 32 || '<>:"|?*'.includes(ch) ? '_' : ch)).join('') || 'file';
    const ext = extname(clean);
    let path = join(dir, clean);
    for (let n = 1; existsSync(path); n++) path = join(dir, `${clean.slice(0, clean.length - ext.length)}-${n}${ext}`);
    writeFileSync(path, Buffer.from(file.base64, 'base64'));
    return path;
  });
}

const MAX_FILES = 50;

// Files created or changed in a chat workspace since `since` (ms), as paths relative to it with forward slashes.
export function newFiles(dir: string, since: number): { path: string; size: number }[] {
  const found: { path: string; size: number }[] = [];
  const walk = (folder: string) => {
    let entries;
    try {
      entries = readdirSync(folder, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (found.length >= MAX_FILES) return;
      const full = join(folder, entry.name);
      // what the user attached is not something the answer created
      if (entry.isDirectory()) {
        if (!(folder === dir && entry.name === 'attachments')) walk(full);
      }
      else if (entry.isFile()) {
        const stat = statSync(full);
        // one second of slack for file systems that round modification times
        if (stat.mtimeMs >= since - 1000) found.push({ path: relative(dir, full).split(sep).join('/'), size: stat.size });
      }
    }
  };
  walk(dir);
  return found;
}

// Resolves a path the UI asks for, refusing anything outside the workspace.
export function insideFolder(dir: string, path: string): string | undefined {
  const root = resolve(dir);
  const full = resolve(root, path);
  return full.startsWith(root + sep) ? full : undefined;
}
