// Is phase-1-base red? A commit pushed onto a red base waits behind the failure, and main cannot follow (R127), so a
// session sees the red run and its failing checks before it pushes (docs/dev/workflow.md#red-base).
//
//   node scripts/base.mjs             prints the red base and what failed, or nothing when it is green or unknown
//   node scripts/base.mjs pre-push    the git hook `cargo t` installs: refuses a push to phase-1-base while it is red,
//                                     or one that carries a number placeholder (scripts/numbers.mjs)
//
// It asks GitHub through `gh` and keeps the answer for three minutes in the shared git folder, so every worktree and every
// `cargo t` reads it once. Offline, signed out or without `gh` it says nothing and never blocks: it is a warning light,
// not a gate. `FABER_PUSH_ON_RED=1 git push` pushes anyway, for the commit that fixes the base.

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { GREP, placeholdersIn, SKIP } from './numbers.mjs';
import { check as owedHere, message as owedMessage, onlyOwed } from './owed.mjs';

const BASE = 'phase-1-base';
const KEEP_MS = 3 * 60 * 1000;
const RED = new Set(['failure', 'timed_out', 'startup_failure']);
const IGNORED = new Set(['cancelled', 'skipped', 'neutral', 'stale']);

/**
 * `{ run, since, newer }` when the newest finished push run of the base failed, `null` when it passed or nothing is known.
 * `runs` is `gh run list --json`, newest first; `since` is the oldest failure of the red streak, `newer` the runs still going.
 */
export function verdict(runs) {
  const finished = runs.filter((r) => r.status === 'completed' && !IGNORED.has(r.conclusion));
  const last = finished[0];
  if (!last || !RED.has(last.conclusion)) return null;
  const streak = finished.findIndex((r) => !RED.has(r.conclusion));
  const since = finished[(streak === -1 ? finished.length : streak) - 1];
  const newer = runs.slice(0, runs.indexOf(last)).filter((r) => r.status !== 'completed');
  return { run: last, since, newer };
}

/** What failed, from `gh run view --log-failed`: the runner's `FAIL` lines and compiler errors, each once, with its job. */
export function failures(log) {
  const found = [];
  for (const line of log.split('\n')) {
    const [job, , rest = ''] = line.split('\t');
    const text = rest.replace(/^﻿?\d{4}-\d\d-\d\dT[\d:.]+Z\s?/, '').trim();
    if (!/^FAIL\s|(^|\s)error(\[\w+\])?:/.test(text)) continue;
    const entry = `${job.trim()}: ${text.replace(/\s+\(\d+ ms\)$/, '').replace(/\s{2,}/g, ' ')}`;
    if (!found.includes(entry)) found.push(entry);
  }
  return found;
}

/** The lines a person reads. `checks` are the failures, or the failed jobs and steps when the log named none. */
export function message(red, checks) {
  const at = (r) => `${r.createdAt.slice(11, 16)} UTC`;
  const lines = [`${BASE} is red since ${at(red.since)}: ${red.run.displayTitle} (${red.run.headSha.slice(0, 7)}), ${red.run.url}`];
  for (const check of checks.slice(0, 6)) lines.push(`  ${check}`);
  if (checks.length > 6) lines.push(`  and ${checks.length - 6} more`);
  for (const run of red.newer) lines.push(`  still running: ${run.displayTitle} (${run.headSha.slice(0, 7)}); if it is the fix, wait for it`);
  lines.push('Fix the base first, or keep your commits until it is green: a push onto it only waits behind the failure.');
  return lines.join('\n');
}

/** Whether a pre-push hook's stdin (`<local ref> <local sha> <remote ref> <remote sha>` per line) updates the base. */
export function pushedToBase(stdin) {
  return stdin
    .split('\n')
    .map((line) => line.trim().split(/\s+/))
    .filter(([, sha, ref]) => ref === `refs/heads/${BASE}` && sha && !/^0+$/.test(sha))
    .map(([, sha]) => sha);
}

const run = (cmd, args, timeout = 10_000) =>
  execFileSync(cmd, args, { encoding: 'utf8', timeout, maxBuffer: 256 << 20, stdio: ['ignore', 'pipe', 'ignore'], windowsHide: true });

/** The red base's lines, or '' when it is green or nothing could be learned. Reads and fills the shared cache. */
function status() {
  let cache;
  try {
    cache = resolve(run('git', ['rev-parse', '--git-common-dir']).trim(), 'faber-base.json');
    const kept = JSON.parse(readFileSync(cache, 'utf8'));
    if (Date.now() - kept.at < KEEP_MS) return kept.text;
  } catch {
    // No cache yet, or an unreadable one: ask again.
  }
  let text;
  try {
    const fields = 'status,conclusion,headSha,databaseId,createdAt,displayTitle,url';
    const runs = JSON.parse(run('gh', ['run', 'list', '--workflow', 'ci.yml', '--branch', BASE, '--event', 'push', '--limit', '20', '--json', fields]));
    const red = verdict(runs);
    if (!red) {
      text = '';
    } else {
      let checks = [];
      try {
        checks = failures(run('gh', ['run', 'view', String(red.run.databaseId), '--log-failed'], 30_000));
      } catch {
        // The log may be gone or too slow; the jobs still say where it broke.
      }
      if (checks.length === 0) {
        const jobs = JSON.parse(run('gh', ['run', 'view', String(red.run.databaseId), '--json', 'jobs'])).jobs;
        checks = jobs
          .filter((j) => RED.has(j.conclusion))
          .map((j) => `${j.name}: ${j.steps.filter((s) => RED.has(s.conclusion)).map((s) => s.name).join(', ') || 'failed'}`);
      }
      text = message(red, checks);
    }
  } catch {
    return ''; // Offline, signed out or no `gh`: nothing is known, and nothing is kept.
  }
  try {
    if (cache) writeFileSync(cache, JSON.stringify({ at: Date.now(), text }));
  } catch {
    // A read-only git folder only costs the next run another question.
  }
  return text;
}

function prePush() {
  let stdin = '';
  try {
    stdin = readFileSync(0, 'utf8');
  } catch {
    // No stdin: nothing is known about the refs, so nothing is refused.
  }
  const shas = pushedToBase(stdin);
  for (const sha of shas) {
    let found = [];
    try {
      found = placeholdersIn(run('git', ['grep', '-I', '-n', '-E', GREP, sha, '--', '.', ...SKIP.map((f) => `:!${f}`)]));
    } catch {
      // `git grep` exits 1 when nothing matches.
    }
    if (found.length) {
      console.error(`Placeholders cannot land on ${BASE}; run \`node scripts/numbers.mjs assign\` first:\n  ${found.join('\n  ')}`);
      return 1;
    }
  }
  const red = status();
  if (red) {
    console.error(red);
    if (shas.length > 0 && process.env.FABER_PUSH_ON_RED !== '1') {
      console.error(`Push refused. If this push is the fix, push it with FABER_PUSH_ON_RED=1.`);
      return 1;
    }
  }
  // A lane pushes only the fixes of what it queued while one is red or left unqueued (docs/dev/workflow.md#owed-work).
  const owed = owedHere();
  if (owed.length === 0 || onlyOwed(stdin, owed)) return 0;
  console.error(owedMessage(owed));
  console.error('Push refused: it is not one of those. FABER_IGNORE_OWED=1 pushes it anyway, for work those wait on.');
  return 1;
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'base.mjs'))) {
  if (process.argv[2] === 'pre-push') {
    process.exitCode = prePush();
  } else {
    const text = status();
    if (text) console.log(text);
    const owed = owedHere();
    if (owed.length) console.log(owedMessage(owed));
  }
}
