import { readdirSync, statSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';

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
      if (entry.isDirectory()) walk(full);
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
