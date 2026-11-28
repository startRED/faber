// node scripts/space.mjs [--json]: what could be removed from Faber's development folders, and why the rest stays.
//
// Read-only. It lists the build folders (the main checkout's shared `target` and each worktree's own) with their size,
// last use and the protection that keeps them, and every registered worktree with its branch, whether its work is on
// origin/phase-1-base, whether it is clean, its age and a verdict. It never removes anything: removing comes later, after
// the owner reads this list (his answer 11 of 2026-11-25). A read that fails gives "unknown", never "would go".

import { execFileSync } from 'node:child_process';
import { existsSync, lstatSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;
/** The build budget the sweep keeps to (R463, `testkit/src/sweep.rs`). */
export const CAP = 24 * 1024 ** 3;
/** A worktree whose work is on the base, clean and untouched this long would go. */
export const OLD = 7 * DAY;
/** The sweep's own times: a cache used within ACTIVE is a lane at work; one idle past IDLE loses its build output. */
const ACTIVE = 30 * 60 * 1000;
const IDLE = 2 * HOUR;
const BASE = 'origin/phase-1-base';

/** `git worktree list --porcelain` as records; the first is the main checkout. */
export function parseWorktrees(porcelain) {
  const records = [];
  for (const block of porcelain.replace(/\r/g, '').split('\n\n')) {
    const lines = block.split('\n').filter(Boolean);
    const field = (name) => lines.find((line) => line === name || line.startsWith(`${name} `));
    const value = (name) => field(name)?.slice(name.length + 1);
    if (!field('worktree')) continue;
    records.push({
      path: value('worktree'),
      head: value('HEAD') ?? null,
      branch: value('branch')?.replace(/^refs\/heads\//, '') ?? null,
      detached: Boolean(field('detached')),
      prunable: Boolean(field('prunable')),
      locked: Boolean(field('locked')),
    });
  }
  return records;
}

/**
 * The verdict on one worktree from what was read about it. `merged`, `clean` and `age` are null when they could not be
 * read. Only a worktree whose work is on the base, that is clean and that nobody touched for a week would go.
 */
export function worktreeVerdict({ main = false, prunable = false, locked = false, merged, clean, age }) {
  if (main) return { verdict: 'keep', why: 'the main checkout' };
  if (locked) return { verdict: 'keep', why: 'locked in git' };
  if (prunable) return { verdict: 'would go', why: 'its folder is gone; only git\'s record of it is left' };
  if (clean === false) return { verdict: 'keep', why: 'uncommitted or untracked changes' };
  if (merged === false) return { verdict: 'keep', why: `work not on ${BASE}` };
  if (merged == null || clean == null || age == null) return { verdict: 'unknown', why: 'could not be read' };
  if (age < OLD) return { verdict: 'keep', why: `used in the last ${Math.round(OLD / DAY)} days` };
  return { verdict: 'would go', why: `on ${BASE}, clean, idle ${Math.floor(age / DAY)} days` };
}

/** The verdict on one build folder, by the same protections the sweep uses. `used` is null when it could not be read. */
export function cacheVerdict({ mainOwn = false, leased = false, used, now }) {
  if (mainOwn) return { verdict: 'keep', why: 'the main checkout\'s own build folder (only ever swept)' };
  if (leased) return { verdict: 'keep', why: 'leased by a build right now' };
  if (used == null) return { verdict: 'unknown', why: 'could not be read' };
  if (now - used < ACTIVE) return { verdict: 'keep', why: 'used in the last half hour (a lane at work)' };
  if (now - used < IDLE) return { verdict: 'keep', why: 'used in the last two hours' };
  return { verdict: 'would go', why: `idle ${Math.floor((now - used) / HOUR)} hours` };
}

/** Reads one worktree through `git`, a function taking arguments and a folder that returns stdout or throws. */
export function readWorktree(record, git, now) {
  const at = (args) => {
    try {
      return git(args, record.path);
    } catch {
      return null;
    }
  };
  if (record.prunable || !existsSync(record.path)) return { ...record, prunable: true, merged: null, clean: null, age: null };
  const status = at(['status', '--porcelain', '--untracked-files=normal']);
  const clean = status == null ? null : status.trim() === '';
  let merged = null;
  if (record.head) {
    if (at(['merge-base', '--is-ancestor', record.head, BASE]) != null) merged = true;
    else {
      // Squashed: every file the branch changed since it left the base is the same on the base now.
      const fork = at(['merge-base', record.head, BASE])?.trim();
      const files = fork ? at(['diff', '--name-only', fork, record.head])?.split('\n').filter(Boolean) : null;
      if (files && files.length === 0) merged = true;
      else if (files) merged = at(['diff', '--quiet', record.head, BASE, '--', ...files]) != null;
    }
  }
  const committed = Number(at(['log', '-1', '--format=%ct', 'HEAD'])?.trim()) * 1000;
  let touched = null;
  try {
    touched = statSync(record.path).mtimeMs;
  } catch {}
  const last = Math.max(Number.isFinite(committed) ? committed : 0, touched ?? 0);
  return { ...record, merged, clean, age: last > 0 ? now - last : null };
}

/** Bytes under a folder, never following a link; unreadable parts make it null. */
export function folderSize(dir) {
  let total = 0;
  const stack = [dir];
  while (stack.length) {
    const here = stack.pop();
    let entries;
    try {
      entries = readdirSync(here, { withFileTypes: true });
    } catch {
      return null;
    }
    for (const entry of entries) {
      const full = path.join(here, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory()) {
        // A junction reads as a folder on Windows: lstat tells it apart.
        try {
          if (lstatSync(full).isSymbolicLink()) continue;
        } catch {
          return null;
        }
        stack.push(full);
      } else if (entry.isFile()) {
        try {
          total += lstatSync(full).size;
        } catch {
          return null;
        }
      }
    }
  }
  return total;
}

/** When a cache was last built in: the newest of its profiles and their `deps`, as the sweep reads it. */
function lastUsed(dir) {
  const times = [];
  try {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      for (const sub of ['', 'deps']) {
        const at = path.join(dir, entry.name, sub);
        if (existsSync(path.join(dir, entry.name, 'deps'))) times.push(statSync(at).mtimeMs);
      }
    }
    if (!times.length) times.push(statSync(dir).mtimeMs);
  } catch {
    return null;
  }
  return Math.max(...times);
}

const leasedIn = (dir) => existsSync(path.join(dir, '.faber-lease')) || existsSync(path.join(path.dirname(dir), '.faber-lease'));

/** The build folders: the shared caches in the main `target`, the main checkout's own, and each worktree's `target`. */
export function readCaches(main, worktrees, now) {
  const caches = [];
  const shared = path.join(main, 'target');
  let names = [];
  try {
    names = readdirSync(shared, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name);
  } catch {}
  for (const name of names) {
    const dir = path.join(shared, name);
    const mainOwn = ['debug', 'release', 'faber-test'].includes(name);
    // The launcher's leased caches (`build-1` to `build-3`) carry no tag of their own: Cargo tags only the folder it made.
    if (!mainOwn && !/^build-\d+$/.test(name) && !existsSync(path.join(dir, 'CACHEDIR.TAG'))) continue;
    caches.push({ dir, size: folderSize(dir), used: lastUsed(dir), ...cacheVerdict({ mainOwn, leased: leasedIn(dir), used: lastUsed(dir), now }) });
  }
  for (const worktree of worktrees.slice(1)) {
    const dir = path.join(worktree.path, 'target');
    if (!existsSync(path.join(dir, 'CACHEDIR.TAG'))) continue;
    caches.push({ dir, size: folderSize(dir), used: lastUsed(dir), ...cacheVerdict({ leased: leasedIn(dir), used: lastUsed(dir), now }) });
  }
  return caches;
}

/** Sums bytes (or counts) per verdict; an unknown size counts as 0 bytes and is named in `unknownSizes`. */
export function totals(items) {
  const sums = { 'would go': { count: 0, bytes: 0 }, keep: { count: 0, bytes: 0 }, unknown: { count: 0, bytes: 0 } };
  let unknownSizes = 0;
  for (const item of items) {
    sums[item.verdict].count += 1;
    if (item.size == null) unknownSizes += 'size' in item ? 1 : 0;
    else sums[item.verdict].bytes += item.size;
  }
  return { ...sums, unknownSizes };
}

/** Why the build folders are over the cap, in one sentence: which protection holds how much. */
export function overCap(caches) {
  const total = caches.reduce((sum, c) => sum + (c.size ?? 0), 0);
  if (total <= CAP) return `Build folders: ${gb(total)} of the ${gb(CAP)} budget.`;
  const held = new Map();
  for (const cache of caches.filter((c) => c.verdict === 'keep')) held.set(cache.why, (held.get(cache.why) ?? 0) + (cache.size ?? 0));
  const reasons = [...held].sort((a, b) => b[1] - a[1]).map(([why, bytes]) => `${gb(bytes)} ${why}`);
  return `Build folders: ${gb(total)}, over the ${gb(CAP)} budget, because the sweep keeps ${reasons.join(', ') || 'nothing'}; ${gb(totals(caches)['would go'].bytes)} would go.`;
}

export const gb = (bytes) => `${(bytes / 1024 ** 3).toFixed(1)} GB`;
const days = (ms) => (ms == null ? '?' : `${Math.floor(ms / DAY)} d`);

function run() {
  const json = process.argv.includes('--json');
  const git = (args, cwd) => execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], windowsHide: true, maxBuffer: 64 * 1024 * 1024 });
  const common = git(['rev-parse', '--path-format=absolute', '--git-common-dir'], process.cwd()).trim();
  const main = path.dirname(common);
  const now = Date.now();
  const records = parseWorktrees(git(['worktree', 'list', '--porcelain'], main));
  const worktrees = records.map((record, index) => {
    const read = readWorktree(record, git, now);
    return { ...read, ...worktreeVerdict({ ...read, main: index === 0 }) };
  });
  const caches = readCaches(main, records, now);
  if (json) {
    console.log(JSON.stringify({ caches, worktrees, cacheTotals: totals(caches), worktreeTotals: totals(worktrees) }, null, 2));
    return;
  }
  console.log('Build folders');
  for (const c of caches.sort((a, b) => (b.size ?? 0) - (a.size ?? 0))) {
    console.log(`  ${c.verdict.padEnd(8)} ${(c.size == null ? '?' : gb(c.size)).padStart(8)}  ${c.dir}  (${c.why})`);
  }
  console.log(`  ${overCap(caches)}`);
  console.log('\nWorktrees');
  for (const w of worktrees) {
    console.log(`  ${w.verdict.padEnd(8)} ${days(w.age).padStart(5)}  ${w.branch ?? (w.detached ? '(detached)' : '?')}  ${w.path}  (${w.why})`);
  }
  const t = totals(worktrees);
  console.log(`\n  ${worktrees.length} worktrees: ${t['would go'].count} would go, ${t.keep.count} keep, ${t.unknown.count} unknown. Nothing was removed.`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) run();
