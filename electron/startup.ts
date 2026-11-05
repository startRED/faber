// How long this start took, one line in `startup.log` (the file the Tauri shell kept, newest 32 KB): when the window was
// built, when it showed (by its first paint or the failsafe), and when the host first answered the screen, so a slow
// start says which side was slow. Nothing else is measured and nothing leaves the PC.

import { appendFileSync, lstatSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

/** The newest bytes kept, as `LOG_MAX` in `update/src/lib.rs`. */
const LOG_MAX = 32 * 1024;
/** A host that has not answered by then is written as not answered, so a start that never gets there still leaves its line. */
const GIVE_UP_MS = 180_000;

type Marks = { window?: number; shown?: number; by?: 'paint' | 'failsafe'; host?: number };

const marks: Marks = {};
let written = false;
let dataDir = '';
let giveUp: NodeJS.Timeout | undefined;

/** Milliseconds since this process started. */
const since = () => Math.round(performance.now());

export function begin(dir: string): void {
  dataDir = dir;
  giveUp = setTimeout(write, GIVE_UP_MS);
  giveUp.unref();
}

export function windowBuilt(): void {
  marks.window ??= since();
}

export function shown(by: 'paint' | 'failsafe'): void {
  if (marks.shown !== undefined) return;
  marks.shown = since();
  marks.by = by;
  if (marks.host !== undefined) write();
}

/** The host answered the screen for the first time. */
export function hostAnswered(): void {
  if (marks.host !== undefined) return;
  marks.host = since();
  if (marks.shown !== undefined) write();
}

/** The line, as the Tauri shell wrote it, with the host's first answer after it. */
export function line(at: number, { window, shown, by, host }: Marks): string {
  const ms = (value: number | undefined) => (value === undefined ? 'none' : `${value}ms`);
  return `${at} shown window=${ms(window)} shown=${ms(shown)} by=${by ?? 'none'} host=${ms(host)}`;
}

/** `log` with `entry` added; past `LOG_MAX` only the newest whole lines stay (as `with_line` in `update/src/lib.rs`). */
export function appended(log: string, entry: string): string {
  const text = `${log}${entry}\n`;
  if (text.length <= LOG_MAX) return text;
  const from = text.length - LOG_MAX / 2;
  const cut = text.indexOf('\n', from);
  return cut < 0 ? '' : text.slice(cut + 1);
}

function write(): void {
  if (written || !dataDir) return;
  written = true;
  clearTimeout(giveUp);
  try {
    const file = path.join(dataDir, 'startup.log');
    const entry = line(Math.floor(Date.now() / 1000), marks);
    let size = 0;
    try {
      const found = lstatSync(file);
      if (found.isSymbolicLink()) return;
      size = found.size;
    } catch {}
    // Added at the end while it fits: the host writes this file too, and a rewrite could drop its line.
    if (size + entry.length + 1 <= LOG_MAX) appendFileSync(file, `${entry}\n`);
    else writeFileSync(file, appended(readFileSync(file, 'utf8'), entry));
  } catch {
    // A start never fails for its own log.
  }
}
