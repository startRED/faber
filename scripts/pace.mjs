// How fast work lands: what entered phase-1-base, how long it took, how the queue ran and what is left to take.
//
//   node scripts/pace.mjs [--json]
//
// For the last 24 hours and the last 7 days: pull requests merged into phase-1-base (land.mjs lands them and CI's land job
// marks them merged), their changed lines (additions + deletions) and their time from opening to merged, both as median
// and p90; the queue rounds (runs of ci.yml on queue/* branches) green, red and cancelled, with the three checks that
// failed most often by chance; and the free tasks now (`tasks.mjs list`). The target is about 80 pull requests a day of
// 100 to 400 changed lines each.

import { execFile, execFileSync } from 'node:child_process';
import { promisify } from 'node:util';
import { resolve, join } from 'node:path';
import { failures } from './base.mjs';

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

/** A check's name in a `failures` entry of a runner `FAIL` line ("ui: FAIL  ui      name (3.1 s)"); null for anything else. */
export function checkName(entry) {
  const m = /^[^:]*: FAIL\s+\S+\s+(.+?)(\s+\([\d.]+ m?s\))?$/.exec(entry);
  return m ? m[1] : null;
}

/** The pull request heads a queue round checks: the `Pull-request-head` trailers of its head commit's message. */
export const headsOf = (message) => [...(message ?? '').matchAll(/^Pull-request-head: (\S+)/gm)].map((m) => m[1]);

/** The red rounds that one of their pull request heads passed at the same time or later: a later queue round, the same
 * run on a rerun, or the pull request landing with that head unchanged. `reds` and `greens` are { heads, createdAt }. */
export const passedLater = (reds, greens) =>
  reds.filter((red) => greens.some((g) => Date.parse(g.createdAt) >= Date.parse(red.createdAt) && g.heads.some((h) => red.heads.includes(h))));

/** The checks that failed by chance, most frequent first, as [check, times]: each red's failed checks (`checks`) count
 * once, and only the reds `passedLater` keeps. A red that never passed again counts for nothing. */
export function flaky(reds, greens, top = 3) {
  const counts = new Map();
  for (const red of passedLater(reds, greens)) for (const check of new Set(red.checks)) counts.set(check, (counts.get(check) ?? 0) + 1);
  return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, top);
}

const num = (n) => (n === null ? '-' : String(Math.round(n)));

/** The short text of one window. */
export function format(name, s, free, chance = []) {
  return [
    `${name}: ${s.landed} pull requests landed`,
    `  lines per pull request: median ${num(s.lines.median)}, p90 ${num(s.lines.p90)}`,
    `  opened to landed (min): median ${num(s.minutes.median)}, p90 ${num(s.minutes.p90)}`,
    `  queue rounds: ${s.rounds.green} green, ${s.rounds.red} red, ${s.rounds.cancelled} cancelled`,
    ...chance.map(([check, n]) => `  flaky: ${check} ×${n}`),
    `  free tasks now: ${free}`,
  ].join('\n');
}

// The thin layer below talks to GitHub and git; the tests never call it.
const gh = (...args) => execFileSync('gh', args, { encoding: 'utf8', maxBuffer: 256 << 20, windowsHide: true });

function fetchAll(now) {
  const since = new Date(now - WINDOWS[1].ms).toISOString();
  const prs = JSON.parse(
    gh('pr', 'list', '--state', 'merged', '--base', 'phase-1-base', '--limit', '3000', '--search', `merged:>=${since.slice(0, 10)}`, '--json', 'number,additions,deletions,createdAt,mergedAt,headRefOid'),
  );
  // Pushes to queue/* are the rounds; --paginate walks every page of the week.
  const runs = gh('api', '--paginate', `repos/{owner}/{repo}/actions/workflows/ci.yml/runs?event=push&per_page=100&created=>=${since.slice(0, 10)}`, '--jq', '.workflow_runs[] | select(.head_branch | startswith("queue/")) | {id, attempt: .run_attempt, createdAt: .created_at, conclusion, heads: .head_commit.message, endedAt: .updated_at}')
    .split('\n')
    .filter(Boolean)
    .map((l) => JSON.parse(l))
    .map((r) => ({ ...r, heads: headsOf(r.heads) }));
  // tasks.mjs owns what free means; its list prints one line per task, 'claimed by' only for a live claim.
  const list = execFileSync('node', [join(import.meta.dirname, 'tasks.mjs'), 'list'], { encoding: 'utf8', windowsHide: true });
  return { prs, runs, free: countFree(list) };
}

// Logs are read only for the reds a later green makes flaky, at most 40 per window, newest first, eight at a time and
// each attempt once for both windows, so the report stays under a minute.
const logs = new Map();
const attemptChecks = (id, attempt) => {
  const key = `${id}/${attempt}`;
  if (!logs.has(key)) {
    const read = promisify(execFile)('gh', ['run', 'view', String(id), '--attempt', String(attempt), '--log-failed'], { maxBuffer: 256 << 20, windowsHide: true });
    // A log GitHub no longer keeps, or an attempt that did not fail, names nothing.
    logs.set(key, read.then(({ stdout }) => failures(stdout).map(checkName).filter(Boolean), () => []));
  }
  return logs.get(key);
};
// A red attempt's checks; a green rerun's are those of every attempt before it (reruns are rare, so this reads little).
const checksOf = async (round) => {
  if (round.conclusion !== 'success') return attemptChecks(round.id, round.attempt);
  const before = Array.from({ length: round.attempt - 1 }, (_, i) => attemptChecks(round.id, i + 1));
  return [...new Set((await Promise.all(before)).flat())];
};
async function chanceIn(rounds, prs) {
  const reds = rounds.filter((r) => r.conclusion === 'failure' || r.conclusion === 'timed_out' || (r.conclusion === 'success' && r.attempt > 1));
  // A green counts from when it ended: a rerun passes long after its run was created.
  const greens = [...rounds.filter((r) => r.conclusion === 'success').map((r) => ({ heads: r.heads, createdAt: r.endedAt })), ...prs.map((p) => ({ heads: [p.headRefOid], createdAt: p.mergedAt }))];
  const read = passedLater(reds, greens).sort((a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt)).slice(0, 40);
  for (let i = 0; i < read.length; i += 8) await Promise.all(read.slice(i, i + 8).map(async (r) => (r.checks = await checksOf(r))));
  return flaky(read, greens);
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'pace.mjs'))) {
  const now = Date.now();
  const { prs, runs, free } = fetchAll(now);
  const all = [];
  for (const w of WINDOWS) {
    all.push({ window: w.name, ...summarize(prs, runs, now, w.ms), flaky: await chanceIn(inWindow(runs, 'createdAt', now, w.ms), prs), free });
  }
  console.log(process.argv.includes('--json') ? JSON.stringify(all, null, 2) : WINDOWS.map((w, i) => format(w.name, all[i], free, all[i].flaky)).join('\n\n'));
}
