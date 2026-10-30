// How fast work lands: what entered phase-1-base, how long it took, how the queue ran and what is left to take.
//
//   node scripts/pace.mjs [--json]
//
// For the last 24 hours and the last 7 days: pull requests merged into phase-1-base (land.mjs lands them and CI's land job
// marks them merged), their changed lines (additions + deletions) and their time from opening to merged, both as median
// and p90; the queue rounds (runs of ci.yml on queue/* branches) green, red and cancelled; and the free tasks now
// (`tasks.mjs list`). The target is about 80 pull requests a day of 100 to 400 changed lines each.

import { execFileSync } from 'node:child_process';
import { resolve, join } from 'node:path';

const HOUR = 60 * 60 * 1000;
export const WINDOWS = [
  { name: '24 h', ms: 24 * HOUR },
  { name: '7 days', ms: 7 * 24 * HOUR },
];

/** Nearest-rank percentile (0 to 100) of numbers; null when there are none. */
export function percentile(values, p) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil((p / 100) * sorted.length) - 1)];
}
export const median = (values) => percentile(values, 50);
export const p90 = (values) => percentile(values, 90);

/** The items whose `key` time lies in the `ms` before `now`. */
export const inWindow = (items, key, now, ms) => items.filter((i) => now - Date.parse(i[key]) <= ms && Date.parse(i[key]) <= now);

/** One window's numbers from plain arrays: `prs` ({ additions, deletions, createdAt, mergedAt }), `runs` ({ createdAt, conclusion }). */
export function summarize(prs, runs, now, ms) {
  const merged = inWindow(prs, 'mergedAt', now, ms);
  const lines = merged.map((p) => p.additions + p.deletions);
  const minutes = merged.map((p) => (Date.parse(p.mergedAt) - Date.parse(p.createdAt)) / 60000);
  const rounds = inWindow(runs, 'createdAt', now, ms);
  const count = (f) => rounds.filter((r) => f(r.conclusion)).length;
  return {
    landed: merged.length,
    lines: { median: median(lines), p90: p90(lines) },
    minutes: { median: median(minutes), p90: p90(minutes) },
    rounds: { green: count((c) => c === 'success'), red: count((c) => c === 'failure' || c === 'timed_out'), cancelled: count((c) => c === 'cancelled') },
  };
}

/** Free tasks in the text of `tasks.mjs list`: unclaimed (`free`) or claimed stale; a task that waits on another is not. */
export const countFree = (text) => text.split(/\r?\n/).filter((l) => /  (free$|stale claim by )/.test(l)).length;

const num = (n) => (n === null ? '-' : String(Math.round(n)));

/** The short text of one window. */
export function format(name, s, free) {
  return [
    `${name}: ${s.landed} pull requests landed`,
    `  lines per pull request: median ${num(s.lines.median)}, p90 ${num(s.lines.p90)}`,
    `  opened to landed (min): median ${num(s.minutes.median)}, p90 ${num(s.minutes.p90)}`,
    `  queue rounds: ${s.rounds.green} green, ${s.rounds.red} red, ${s.rounds.cancelled} cancelled`,
    `  free tasks now: ${free}`,
  ].join('\n');
}

// The thin layer below talks to GitHub and git; the tests never call it.
const gh = (...args) => execFileSync('gh', args, { encoding: 'utf8', maxBuffer: 256 << 20, windowsHide: true });

function fetchAll(now) {
  const since = new Date(now - WINDOWS[1].ms).toISOString();
  const prs = JSON.parse(
    gh('pr', 'list', '--state', 'merged', '--base', 'phase-1-base', '--limit', '3000', '--search', `merged:>=${since.slice(0, 10)}`, '--json', 'number,additions,deletions,createdAt,mergedAt'),
  );
  // Pushes to queue/* are the rounds; --paginate walks every page of the week.
  const runs = gh('api', '--paginate', `repos/{owner}/{repo}/actions/workflows/ci.yml/runs?event=push&per_page=100&created=>=${since.slice(0, 10)}`, '--jq', '.workflow_runs[] | select(.head_branch | startswith("queue/")) | {createdAt: .created_at, conclusion}')
    .split('\n')
    .filter(Boolean)
    .map((l) => JSON.parse(l));
  // tasks.mjs owns what free means; its list prints one line per task, 'claimed by' only for a live claim.
  const list = execFileSync('node', [join(import.meta.dirname, 'tasks.mjs'), 'list'], { encoding: 'utf8', windowsHide: true });
  return { prs, runs, free: countFree(list) };
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'pace.mjs'))) {
  const now = Date.now();
  const { prs, runs, free } = fetchAll(now);
  const all = WINDOWS.map((w) => ({ window: w.name, ...summarize(prs, runs, now, w.ms), free }));
  console.log(process.argv.includes('--json') ? JSON.stringify(all, null, 2) : WINDOWS.map((w, i) => format(w.name, all[i], free)).join('\n\n'));
}
