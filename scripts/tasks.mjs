// The task queue (docs/dev/workflow.md#task-queue): what a session does next when it finishes a task, without asking.
//
//   node scripts/tasks.mjs next [--who <name>]   claims the first free task on origin/phase-1-base and prints its brief
//   node scripts/tasks.mjs list                  every task, in order, with who holds it
//   node scripts/tasks.mjs release <task>        gives a claimed task back
//   node scripts/tasks.mjs board [--push]        the board: doing, landing, next, waiting and landed, as Markdown;
//                                                --push publishes it as BOARD.md, the only file of the branch `board`
//
// A task is a file of its own, docs/tasks/<order>-<name>.md: a `# Title` line, then the brief a fresh session needs. The
// order is a number (`010`, `020`), lowest first, so adding, reordering or finishing one never touches another's lines.
// A session claims one by creating the branch claim/<name> on GitHub: a push without force fails when the branch is
// already there, so two sessions never get the same task. The claim is a commit of its own whose message says who and
// when; one older than a day is stale and the next session takes it over. The pull request that finishes a task deletes
// its file, so the task is gone once it lands, and the next `next` deletes its claim. A `Depends: a, b` line names tasks
// that land first: `next` skips a task while one of them is still on the list. The board (docs/dev/workflow.md#board) is
// built from the same state after every claim, release and landing, so nobody keeps a progress file by hand.

import { execFileSync } from 'node:child_process';
import { resolve, join } from 'node:path';
import { check, message } from './owed.mjs';

const BASE = 'origin/phase-1-base';
export const TASKS = 'docs/tasks';
/** A claim nobody renewed for this long is stale (the same day as lane claims, docs/dev/workflow.md#lane-claims). */
export const STALE_MS = 24 * 60 * 60 * 1000;
const FILE = /^(\d+)-([a-z0-9][a-z0-9-]*)\.md$/;

/** How many rows each list of the board shows, so a person and an agent read it whole: the rest is counted. */
export const BOARD_ROWS = 10;

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

/** The task names on a `Depends:` line of a task file's text; `none` or no line is none. */
export function parseDepends(text) {
  const line = /^Depends:(.*)$/m.exec(text)?.[1] ?? '';
  return line.split(',').map((d) => d.trim()).filter((d) => d && d !== 'none');
}

/**
 * The tasks in the order a session takes them, each with its claim: `files` are the names in docs/tasks, `claims` maps a
 * task name to `{ who, since }`, `depends` a task name to the names it waits on. `waits` are those still on the list;
 * `free` is true for a task that waits on none and is unclaimed or claimed stale at `now`.
 */
export function queue(files, claims, now, depends = new Map()) {
  const tasks = files.map(parseName).filter(Boolean);
  const listed = new Set(tasks.map((t) => t.name));
  return tasks
    .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name))
    .map((task) => {
      const claim = claims.get(task.name);
      const waits = (depends.get(task.name) ?? []).filter((d) => listed.has(d));
      return { ...task, claim, waits, free: !waits.length && (!claim || now - claim.since > STALE_MS) };
    });
}

/** Claims whose task is gone from the list: finished, so their branch can go. */
export const finished = (files, claims) => {
  const names = new Set(files.map(parseName).filter(Boolean).map((t) => t.name));
  return [...claims.keys()].filter((name) => !names.has(name));
};

/** A duration as a person reads it: `12 min`, `3 h`, `2 d`. */
export function ago(ms) {
  const min = Math.max(0, Math.round(ms / 60_000));
  if (min < 60) return `${min} min`;
  return min < 48 * 60 ? `${Math.round(min / 60)} h` : `${Math.round(min / 1440)} d`;
}

/** The GitHub page of a remote URL (`https://github.com/o/r.git`, `git@github.com:o/r.git`), or '' for another host. */
export function repoUrl(remote) {
  const m = /github\.com[:/]([^/\s]+\/[^/\s]+?)(?:\.git)?\/?$/.exec(remote.trim());
  return m ? `https://github.com/${m[1]}` : '';
}

/** A commit subject with its trailing `(#123)` linked to the pull request when the repository is known. */
const linked = (subject, repo) => (repo ? subject.replace(/\(#(\d+)\)$/, `([#$1](${repo}/pull/$1))`) : subject);

/**
 * The board as Markdown: what is being done, what is landing, what comes next, what waits and what landed. `tasks` is
 * `queue()`'s output, each with its `title` and `area`; `landing` the queued commits `{ subject, since }`; `done` the
 * last landings `{ date, subject }`, newest first. Every list stops at BOARD_ROWS, so its size never grows with the queue.
 */
export function board({ tasks, landing, done, now, repo = '' }) {
  const label = (t) => `**${t.title || t.name}** (\`${t.name}\`)`;
  const fresh = (t) => t.claim && now - t.claim.since <= STALE_MS;
  const doing = tasks.filter(fresh);
  const next = tasks.filter((t) => t.free);
  const waiting = tasks.filter((t) => !t.free && !fresh(t));
  const list = (items, row, empty) => {
    const rows = items.slice(0, BOARD_ROWS).map(row);
    if (items.length > BOARD_ROWS) rows.push(`- and ${items.length - BOARD_ROWS} more`);
    return rows.length ? rows.join('\n') : empty;
  };
  const who = (claim) => claim.who.slice(0, 40);
  return [
    '# Board',
    '',
    'Built by `node scripts/tasks.mjs board` from the task list and git after every claim, release and landing; nobody',
    `edits it. Updated ${new Date(now).toISOString().slice(0, 16).replace('T', ' ')} UTC.`,
    '',
    `**${doing.length} doing · ${landing.length} landing · ${next.length} next · ${waiting.length} waiting**`,
    '',
    '## Doing',
    '',
    list(doing, (t) => `- ${label(t)}, ${who(t.claim)}, for ${ago(now - t.claim.since)}`, 'Nobody holds a task.'),
    '',
    '## Landing',
    '',
    list(landing, (c) => `- ${linked(c.subject, repo)}, queued ${ago(now - c.since)} ago`, 'Nothing is being checked to land.'),
    '',
    '## Next',
    '',
    list(next, (t) => `- ${label(t)}${t.area ? `, ${t.area}` : ''}${t.claim ? `, claim by ${who(t.claim)} gone stale` : ''}`, 'No free task.'),
    '',
    '## Waiting on other tasks',
    '',
    list(waiting, (t) => `- ${label(t)} waits on ${t.waits.map((w) => `\`${w}\``).join(', ')}`, 'None.'),
    '',
    '## Landed',
    '',
    list(done, (c) => `- ${c.date}: ${linked(c.subject, repo)}`, 'Nothing yet.'),
    '',
  ].join('\n');
}

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
  // Every Depends, title and Area line in one call: `<base>:docs/tasks/<file>:Depends: a, b`.
  const depends = new Map();
  const titles = new Map();
  const areas = new Map();
  let lines = '';
  try {
    // A person's grep.lineNumber or grep.column would put numbers in front of the text.
    lines = git('-c', 'grep.lineNumber=false', '-c', 'grep.column=false', 'grep', '-e', '^Depends:', '-e', '^# ', '-e', '^Area:', base, '--', TASKS);
  } catch {
    // No task has one of them: git grep exits 1.
  }
  for (const line of lines.split('\n').filter(Boolean)) {
    const [, path, ...rest] = line.split(':');
    const name = parseName(path.slice(TASKS.length + 1))?.name;
    const text = rest.join(':');
    if (!name) continue;
    if (text.startsWith('Depends:')) depends.set(name, parseDepends(text));
    else if (text.startsWith('Area:')) areas.has(name) || areas.set(name, text.slice(5).trim());
    else titles.has(name) || titles.set(name, text.slice(2).trim());
  }
  const claims = new Map();
  const refs = git('for-each-ref', '--format=%(refname:strip=4)%09%(objectname)', 'refs/remotes/origin/claim/');
  for (const line of refs.split('\n').filter(Boolean)) {
    const [name, oid] = line.split('\t');
    claims.set(name, { ...readClaim(git('log', '-1', '--format=%B', oid)), oid, from: git('log', '-1', '--format=%P', oid).trim() });
  }
  return { base, files, claims, depends, titles, areas };
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
  const owed = check();
  if (owed.length) {
    console.log(message(owed));
    console.log('No new task until they are fixed. FABER_IGNORE_OWED=1 takes one anyway, for work those wait on.');
    return 1;
  }
  const { base, files, claims, depends } = state();
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
  for (const task of queue(files, claims, Date.now(), depends)) {
    if (!task.free || !claim(base, task.name, who, task.claim?.oid)) continue;
    const file = files.find((f) => parseName(f)?.name === task.name);
    const { title, brief } = parseTask(git('show', `${base}:${TASKS}/${file}`));
    console.log(`Task ${task.name}: ${title}\n\n${brief}\n`);
    refresh();
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
  const { files, claims, depends } = state();
  const now = Date.now();
  for (const task of queue(files, claims, now, depends)) {
    const fresh = task.claim && now - task.claim.since <= STALE_MS;
    const held = fresh ? `claimed by ${task.claim.who}` : task.waits.length ? `waits on ${task.waits.join(', ')}` : task.claim ? `stale claim by ${task.claim.who}` : 'free';
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
  refresh();
  console.log(`${name} is free again.`);
  return 0;
}

/** The board of this moment, read from GitHub: the task list and claims as `next` sees them, queued and landed commits. */
function current() {
  const { base, files, claims, depends, titles, areas } = state();
  git('fetch', '--quiet', '--prune', 'origin', '+refs/heads/queue/*:refs/remotes/origin/queue/*');
  const now = Date.now();
  const tasks = queue(files, claims, now, depends).map((t) => ({ ...t, title: titles.get(t.name) ?? '', area: areas.get(t.name) ?? '' }));
  const rows = (text) => text.split('\n').filter(Boolean).map((line) => [line.split('\t')[0], line.split('\t').slice(1).join('\t')]);
  const queued = git('for-each-ref', '--sort=-committerdate', '--format=%(committerdate:unix)%09%(subject)', 'refs/remotes/origin/queue/');
  const landing = rows(queued).map(([at, subject]) => ({ since: Number(at) * 1000, subject }));
  const done = rows(git('log', '--first-parent', `-${BOARD_ROWS}`, '--format=%cs%x09%s', base)).map(([date, subject]) => ({ date, subject }));
  return board({ tasks, landing, done, now, repo: repoUrl(git('remote', 'get-url', 'origin')) });
}

/**
 * Publishes the board as BOARD.md, the only file of the branch `board`: one commit with no parent, so the branch never
 * grows and two writers never conflict. shortcut: the last writer wins, so the board can lag a claim made meanwhile by a
 * few seconds; the next claim, release or landing rewrites it.
 */
function publish(text) {
  const run = (args, input) => execFileSync('git', args, { input, encoding: 'utf8', windowsHide: true }).trim();
  const blob = run(['hash-object', '-w', '--stdin'], text);
  const tree = run(['mktree'], `100644 blob ${blob}\tBOARD.md\n`);
  const commit = run(['commit-tree', tree, '-F', '-'], 'board\n');
  git('push', '--quiet', '--force', 'origin', `${commit}:refs/heads/board`);
}

/** After a claim or a release the board follows; a board that fails to publish never undoes the claim. */
function refresh() {
  try {
    publish(current());
  } catch (e) {
    console.error(`The board was not updated: ${e.message.split('\n')[0]}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'tasks.mjs'))) {
  const [command, ...rest] = process.argv.slice(2);
  const at = rest.indexOf('--who');
  const who = at >= 0 ? rest[at + 1] : process.env.CLAUDE_SESSION_ID || `${process.env.USERNAME ?? process.env.USER ?? 'someone'} ${process.pid}`;
  if (command === 'next') process.exitCode = next(who);
  else if (command === 'list') process.exitCode = list();
  else if (command === 'release' && rest[0]) process.exitCode = release(rest[0]);
  else if (command === 'board') rest.includes('--push') ? publish(current()) : console.log(current());
  else {
    console.error('usage: node scripts/tasks.mjs next [--who <name>] | list | release <task> | board [--push]');
    process.exitCode = 2;
  }
}
