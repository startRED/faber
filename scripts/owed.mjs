// Pull requests this worktree queued that have not landed and are not on their way: the queue run failed, or the pull
// request changed since and was never queued again. A lane fixes what it queued before it starts something new
// (docs/dev/workflow.md#owed-work): on 2026-10-28 one lane had six red pull requests from 2026-10-25 and kept adding new
// work on top of them for three days, so none of it could land.
//
//   node scripts/owed.mjs     prints the owed pull requests, or nothing
//
// `land.mjs` records each pull request it queues in this worktree's own git folder. `base.mjs` prints the owed ones on
// every `cargo t`, its pre-push hook refuses a push of any other branch, and `tasks.mjs next` and `land.mjs` of another
// pull request refuse too. Offline, signed out or without `gh` nothing is known and nothing is refused.
// `FABER_IGNORE_OWED=1` goes past it, for work the owed pull requests wait on.

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

export const FILE = 'faber-queued.json';
const RED = new Set(['FAILURE', 'ERROR']);
const LIMIT = 500;

/** `list` with pull request `number` from `branch` in it once, as the newest entry. */
export function record(list, number, branch) {
  return [...list.filter((entry) => entry.number !== number), { number, branch }];
}

/**
 * What `list` still owes, given the open pull requests (`gh pr list --json number,headRefName,statusCheckRollup`):
 * `owed` holds each open one whose `queue` status on its head is red, or missing because it was pushed after queueing;
 * `kept` drops the ones no longer open, which landed or were closed, unless `partial`: `open` was cut at its limit, so a
 * missing one may still be open.
 */
export function owed(list, open, partial = false) {
  const byNumber = new Map(open.map((pr) => [pr.number, pr]));
  const kept = partial ? list : list.filter((entry) => byNumber.has(entry.number));
  const found = [];
  for (const entry of kept) {
    const pr = byNumber.get(entry.number);
    if (!pr) continue;
    const queue = (pr.statusCheckRollup ?? []).find((check) => check.context === 'queue');
    if (queue && !RED.has(queue.state)) continue;
    found.push({ number: pr.number, branch: pr.headRefName, why: queue ? 'red' : 'unqueued' });
  }
  return { owed: found, kept };
}

/** The lines a lane reads. */
export function message(found) {
  const lines = ['This worktree queued pull requests that have not landed:'];
  for (const pr of found) {
    lines.push(`  #${pr.number} ${pr.branch}: ${pr.why === 'red' ? 'its queue run failed' : 'it changed after it was queued and was not queued again'}`);
  }
  lines.push('Fix each and queue it again with `node scripts/land.mjs <number>`, or close the ones given up, before new work.');
  return lines.join('\n');
}

/** Whether a pre-push hook's stdin only pushes the owed pull requests' own branches, or deletes. */
export function onlyOwed(stdin, found) {
  const allowed = new Set(found.flatMap((pr) => [`refs/heads/${pr.branch}`, `refs/heads/queue/pr-${pr.number}`, `refs/heads/verify/pr-${pr.number}`]));
  return stdin
    .split('\n')
    .map((line) => line.trim().split(/\s+/))
    .filter(([local, sha, ref]) => local && ref)
    .every(([, sha, ref]) => /^0+$/.test(sha) || allowed.has(ref));
}

const run = (cmd, args) =>
  execFileSync(cmd, args, { encoding: 'utf8', timeout: 15_000, maxBuffer: 64 << 20, stdio: ['ignore', 'pipe', 'ignore'], windowsHide: true });

function file() {
  return resolve(run('git', ['rev-parse', '--git-dir']).trim(), FILE);
}

function read(path) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch {
    return [];
  }
}

/** Adds pull request `number` from `branch` to this worktree's record. A record that cannot be written costs only the guard. */
export function remember(number, branch) {
  try {
    const path = file();
    writeFileSync(path, JSON.stringify(record(read(path), number, branch)));
  } catch {
    // No git folder or a read-only one: this pull request goes unguarded.
  }
}

/** This worktree's owed pull requests, `[]` when none or nothing could be learned. Forgets the ones that left. */
export function check() {
  if (process.env.FABER_IGNORE_OWED === '1') return [];
  try {
    const path = file();
    const list = read(path);
    if (list.length === 0) return [];
    const open = JSON.parse(run('gh', ['pr', 'list', '--state', 'open', '--limit', String(LIMIT), '--json', 'number,headRefName,statusCheckRollup']));
    const { owed: found, kept } = owed(list, open, open.length >= LIMIT);
    if (kept.length !== list.length) writeFileSync(path, JSON.stringify(kept));
    return found;
  } catch {
    return [];
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'owed.mjs'))) {
  const found = check();
  if (found.length) console.log(message(found));
}
