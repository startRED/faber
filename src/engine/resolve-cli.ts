import { existsSync, readFileSync } from 'node:fs';
import { posix, win32 } from 'node:path';

export interface Cli {
  command: string;
  prefix: string[];
}

interface Env {
  PATH?: string;
  Path?: string;
}

// Finds a CLI on PATH without a shell. On Windows, npm installs `.cmd` shims, which cannot be spawned without a
// shell, so Faber reads the shim and runs what it points to: a native .exe, or `node script.js`.
export function resolveCli(
  name: string,
  env: Env = process.env,
  platform: NodeJS.Platform = process.platform,
  exists: (path: string) => boolean = existsSync,
  read: (path: string) => string = (path) => readFileSync(path, 'utf8'),
): Cli | undefined {
  const p = platform === 'win32' ? win32 : posix;
  const dirs = (env.PATH ?? env.Path ?? '').split(p.delimiter).filter(Boolean);
  const exts = platform === 'win32' ? ['.exe', '.cmd'] : [''];
  for (const dir of dirs) {
    for (const ext of exts) {
      const path = p.join(dir, name + ext);
      if (!exists(path)) continue;
      if (ext !== '.cmd') return { command: path, prefix: [] };
      const targets = [...read(path).matchAll(/"%~?dp0%?\\?([^"%]+)"/gi)].map((m) => p.join(dir, m[1]!));
      const exe = targets.findLast((t) => /\.exe$/i.test(t) && !/node\.exe$/i.test(t));
      if (exe && exists(exe)) return { command: exe, prefix: [] };
      const script = targets.findLast((t) => /\.[cm]?js$/i.test(t));
      const node = [dir, ...dirs].map((d) => p.join(d, 'node.exe')).find(exists);
      if (script && node) return { command: node, prefix: [script] };
    }
  }
  return undefined;
}
