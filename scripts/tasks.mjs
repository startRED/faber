// The task queue (docs/dev/workflow.md#task-queue): what a session does next when it finishes a task, without asking.
//
//   node scripts/tasks.mjs next [--who <name>]   claims the first free task on origin/phase-1-base and prints its brief
//   node scripts/tasks.mjs list                  every task, in order, with who holds it
//   node scripts/tasks.mjs release <task>        gives a claimed task back
//
// A task is a file of its own, docs/tasks/<order>-<name>.md: a `# Title` line, then the brief a fresh session needs. The
// order is a number (`010`, `020`), lowest first, so adding, reordering or finishing one never touches another's lines.
// A session claims one by creating the branch claim/<name> on GitHub: a push without force fails when the branch is
// already there, so two sessions never get the same task. The claim is a commit of its own whose message says who and
// when; one older than a day is stale and the next session takes it over. The pull request that finishes a task deletes
// its file, so the task is gone once it lands, and the next `next` deletes its claim.

import { execFileSync } from 'node:child_process';
import { resolve, join } from 'node:path';

const BASE = 'origin/phase-1-base';
export const TASKS = 'docs/tasks';
/** A claim nobody renewed for this long is stale (the same day as lane claims, docs/dev/workflow.md#lane-claims). */
export const STALE_MS = 24 * 60 * 60 * 1000;
const FILE = /^(\d+)-([a-z0-9][a-z0-9-]*)\.md$/;

/** `{ order, name }` of a task file name, or null when it is not one (`README.md`, a draft without an order). */
export function parseName(file) {
  const m = FILE.exec(file);
  return m ? { order: Number(m[1]), name: m[2] } : null;
}

/** `{ title, brief }` of a task file's text: its first `# ` line and everything after it. */
export function parseTask(text) {
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  const at = lines.findIndex((l) => l.startsWith('# '));
  if (at < 0) return { title: '', brief: text.trim() };
  return { title: lines[at].slice(2).trim(), brief: lines.slice(at + 1).join('\n').trim() };
}

/** The message of a claim commit, and what it says back: who holds the task and since when. */
export const claimMessage = (name, who, at) => `claim ${name}\n\nWho: ${who}\nSince: ${at.toISOString()}\n`;
export function readClaim(message) {
  const who = /^Who: (.+)$/m.exec(message)?.[1] ?? '';
  const since = Date.parse(/^Since: (.+)$/m.exec(message)?.[1] ?? '');
  return { who, since: Number.isNaN(since) ? 0 : since };
}

/**
 * The tasks in the order a session takes them, each with its claim: `files` are the names in docs/tasks, `claims` maps a
 * task name to `{ who, since }`. `free` is true for an unclaimed task and for one whose claim is stale at `now`.
 */
export function queue(files, claims, now) {
  return files
    .map(parseName)
    .filter(Boolean)
    .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name))
    .map((task) => {
      const claim = claims.get(task.name);
      return { ...task, claim, free: !claim || now - claim.since > STALE_MS };
    });
}

/** Claims whose task is gone from the list: finished, so their branch can go. */
export const finished = (files, claims) => {
  const names = new Set(files.map(parseName).filter(Boolean).map((t) => t.name));
  return [...claims.keys()].filter((name) => !names.has(name));
};

const git = (...args) => execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });

function state() {
  git('fetch', '--quiet', '--prune', 'origin', '+refs/heads/phase-1-base:refs/remotes/origin/phase-1-base', '+refs/heads/claim/*:refs/remotes/origin/claim/*');
  // One base for the whole run: another worktree may fetch a newer one meanwhile.
  const base = git('rev-parse', BASE).trim();
  let files = [];
  try {
    files = git('ls-tree', '--name-only', `${base}:${TASKS}`).split('\n').filter(Boolean);
  } catch {
    // No docs/tasks on the base yet: an empty list.
  }
  const claims = new Map();
  const refs = git('for-each-ref', '--format=%(refname:strip=4)%09%(objectname)', 'refs/remotes/origin/claim/');
  for (const line of refs.split('\n').filter(Boolean)) {
    const [name, oid] = line.split('\t');
    claims.set(name, { ...readClaim(git('log', '-1', '--format=%B', oid)), oid, from: git('log', '-1', '--format=%P', oid).trim() });
  }
  return { base, files, claims };
}

/** Pushes a claim of `name` for `who`: only where there was none, or over the stale one `old`. True when it is ours. */
function claim(base, name, who, old) {
  const commit = execFileSync('git', ['commit-tree', `${base}^{tree}`, '-p', base, '-F', '-'], {
    input: claimMessage(name, who, new Date()),
    encoding: 'utf8',
    windowsHide: true,
  }).trim();
  try {
    git('push', '--quiet', `--force-with-lease=refs/heads/claim/${name}:${old ?? ''}`, 'origin', `${commit}:refs/heads/claim/${name}`);
    return true;
  } catch {
    return false; // Another session claimed it first.
  }
}

function next(who) {
  const { base, files, claims } = state();
  for (const name of finished(files, claims)) {
    // Finished only if this list is at least as new as the base the claim was made on: a task added after this run's
    // base, and claimed on it, is not missing from the list but newer than it.
    if (!isAncestor(claims.get(name).from, base)) continue;
    try {
      git('push', '--quiet', `--force-with-lease=refs/heads/claim/${name}:${claims.get(name).oid}`, 'origin', `:refs/heads/claim/${name}`);
    } catch {
      // Someone else cleaned it up first.
    }
  }
  for (const task of queue(files, claims, Date.now())) {
    if (!task.free || !claim(base, task.name, who, task.claim?.oid)) continue;
    const file = files.find((f) => parseName(f)?.name === task.name);
    const { title, brief } = parseTask(git('show', `${base}:${TASKS}/${file}`));
    console.log(`Task ${task.name}: ${title}\n\n${brief}\n`);
    console.log(`Claimed as claim/${task.name}. Work on a branch named ${task.name}; its pull request deletes ${TASKS}/${file}.`);
    return 0;
  }
  console.log('No free task: the list is empty or every task is claimed.');
  return 0;
}

/** Whether commit `a` is in the history of `b`. */
function isAncestor(a, b) {
  try {
    git('merge-base', '--is-ancestor', a, b);
    return true;
  } catch {
    return false;
  }
}

function list() {
  const { files, claims } = state();
  const now = Date.now();
  for (const task of queue(files, claims, now)) {
    const held = task.claim ? `${task.free ? 'stale claim by' : 'claimed by'} ${task.claim.who}` : 'free';
    console.log(`${String(task.order).padStart(3, '0')} ${task.name}  ${held}`);
  }
  return 0;
}

function release(name) {
  const { claims } = state();
  const held = claims.get(name);
  if (!held) {
    console.error(`Nobody holds ${name}.`);
    return 1;
  }
  git('push', '--quiet', `--force-with-lease=refs/heads/claim/${name}:${held.oid}`, 'origin', `:refs/heads/claim/${name}`);
  console.log(`${name} is free again.`);
  return 0;
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'tasks.mjs'))) {
  const [command, ...rest] = process.argv.slice(2);
  const at = rest.indexOf('--who');
  const who = at >= 0 ? rest[at + 1] : process.env.CLAUDE_SESSION_ID || `${process.env.USERNAME ?? process.env.USER ?? 'someone'} ${process.pid}`;
  if (command === 'next') process.exitCode = next(who);
  else if (command === 'list') process.exitCode = list();
  else if (command === 'release' && rest[0]) process.exitCode = release(rest[0]);
  else {
    console.error('usage: node scripts/tasks.mjs next [--who <name>] | list | release <task>');
    process.exitCode = 2;
  }
}
