// Lands a pull request through the queue (R162): squashed into one commit on top of the newest phase-1-base and numbered
// there, so its numbers are given only when it lands and two pull requests never wait on each other's
// (docs/dev/workflow.md#numbers).
//
//   node scripts/land.mjs <pull request> [--dry-run] [--no-verify] [--alone] [--head <commit>]
//
// It squashes the pull request onto origin/phase-1-base without touching the working tree (`git merge-tree`), names the
// commit after the pull request's title and number, numbers it in a throwaway worktree (`node scripts/numbers.mjs assign
// --amend`: docs/next/*.json fold into the locales and the decisions, and every placeholder takes the next free number),
// and pushes it to queue/pr-<number>. CI checks that very commit; the land job of ci.yml puts it on phase-1-base, moves the
// pull request's branch to it so GitHub shows it merged, and lets main follow. When the base moves while the checks run,
// the land job runs this again with --head, the head it squashed before, so the numbers are given again on the new base
// instead of a rebase that would collide. A conflict with today's base stops here, with its files. --dry-run shows the
// commit it would queue and pushes nothing.
//
// Batches (R190 V3): a pull request that touches other parts of the repository than one queued and not landed yet is
// squashed on top of that one instead of the base (at most `BATCH` deep), so one run checks them together and the land
// job (`--land`) lands them together. A red batch is split: the report job (`--split`) queues the top one again alone,
// and each side then answers for itself, so the culprit is the one whose own check is red. `--alone` never batches.
//
// Before it queues, it runs on this PC the suites CI would run for the change (`cargo t --suite <name>`, each whole) on
// the very commit it queues, in one worktree kept for that (R190 V2). When they pass, it posts the commit status
// `faber/verify` with that tree and those suites, and CI leaves them out; the lints still run there. With --no-verify,
// on CI (the land job's run again on a new base), with no `cargo` here, or while another landing checks on this PC, it
// queues without one and CI runs every suite as before. A failure here queues nothing.

import { execFileSync } from 'node:child_process';
import { existsSync, linkSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const BASE = 'phase-1-base';

/**
 * The commit message of a landed pull request: its title and number, its body without the template's comments, and the
 * head it was squashed from. The land job moves the branch from that head only, so a later push stops the landing.
 */
export function message(title, number, body, head) {
  const text = (body ?? '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/[ \t]+$/gm, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
  return `${title} (#${number})${text ? `\n\n${text}` : ''}\n\nPull-request-head: ${head}\n`;
}

/** The files `git merge-tree --write-tree --name-only --no-messages` lists after the tree when the merge conflicts. */
export const conflicts = (output) => [...new Set(output.split('\n').slice(1).map((l) => l.trim()).filter(Boolean))];

/** Files whose conflicts land settles itself: words only, where two lanes adding rows or paragraphs at one place is
 * the usual case and never a contradiction (docs/dev/workflow.md#numbers). */
export const WORDS = /\.md$/;

/**
 * `text` (merged with diff3 markers) with every conflict in which neither side changed a line of the base settled as
 * the base's new lines, then the pull request's; null when a conflict changed a line the base had, which only a person
 * or the lane can settle.
 */
export function insertions(text) {
  const lines = text.split('\n');
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    if (!lines[i].startsWith('<<<<<<< ')) {
      out.push(lines[i]);
      continue;
    }
    const sides = [[], [], []];
    let side = 0;
    for (i++; i < lines.length; i++) {
      // A file with CRLF lines gets CRLF markers: a marker is read without its \r, a line is kept as it is.
      const line = lines[i].replace(/\r$/, '');
      if (side === 0 && line.startsWith('||||||| ')) side = 1;
      else if (side === 1 && line === '=======') side = 2;
      else if (side === 2 && line.startsWith('>>>>>>> ')) break;
      else sides[side].push(lines[i]);
    }
    if (i >= lines.length || side !== 2 || sides[1].length) return null;
    out.push(...sides[0], ...sides[2]);
  }
  return out.join('\n');
}

/**
 * The tree of a squash that `git merge-tree` left in conflict, settled: every conflict is a content conflict in a file of
 * words where both sides only added lines. `output` is merge-tree's, messages included. Returns `{ tree }`, or
 * `{ refused }` with the files nobody may settle by machine.
 */
export function settle(output) {
  const [tree, ...rest] = output.split('\n');
  const files = conflicts(output.split('\n\n')[0]);
  // Every conflict must be a content conflict in a listed file of words: a rename, a deletion, a file moved into a new
  // folder or a folder in the way says something else, and nothing settles it by machine.
  const content = /^CONFLICT \(content\): Merge conflict in (.+)$/;
  const records = rest.slice(rest.indexOf('') + 1).filter((l) => l.startsWith('CONFLICT'));
  const named = new Set(records.map((l) => content.exec(l)?.[1]).filter(Boolean));
  const odd = records.filter((l) => !content.test(l)).map((l) => l.replace(/^CONFLICT /, ''));
  const refused = [...files.filter((f) => !WORDS.test(f) || !named.has(f)), ...odd];
  if (!files.length && !odd.length) refused.push('a conflict git did not name a file for');
  if (refused.length) return { refused };
  const index = join(mkdtempSync(join(tmpdir(), 'faber-index-')), 'index');
  const env = { ...process.env, GIT_INDEX_FILE: index };
  try {
    run('git', ['read-tree', tree.trim()], { env });
    for (const file of files) {
      const settled = insertions(git('cat-file', '-p', `${tree.trim()}:${file}`));
      if (settled === null) refused.push(file);
      else {
        const blob = run('git', ['hash-object', '-w', '--no-filters', '--stdin'], { input: settled, stdio: ['pipe', 'pipe', 'pipe'] }).trim();
        run('git', ['update-index', '--cacheinfo', `100644,${blob},${file}`], { env });
      }
    }
    return refused.length ? { refused } : { tree: run('git', ['write-tree'], { env }).trim() };
  } finally {
    rmSync(dirname(index), { recursive: true, force: true });
  }
}

/** Why this pull request cannot land, or nothing. `pr` is `gh pr view --json state,isDraft,baseRefName,isCrossRepository`. */
export function refusal(pr) {
  if (pr.state !== 'OPEN') return `it is ${pr.state.toLowerCase()}`;
  if (pr.isDraft) return 'it is a draft';
  // The land job marks a pull request merged by moving its branch, which only a branch of this repository allows.
  if (pr.isCrossRepository) return 'it comes from a fork';
  if (pr.baseRefName !== BASE) return `it targets ${pr.baseRefName}, not ${BASE}`;
  return '';
}

/**
 * Why the PC checks nothing before queueing, or nothing: `env` is the process's, `args` the command line's. Only
 * Windows checks: the screen suite runs there alone, and elsewhere `cargo t` skips it and still exits 0.
 */
export function noCheck(env, args, platform) {
  if (args.includes('--dry-run')) return 'a dry run';
  if (args.includes('--no-verify')) return '--no-verify';
  if (env.GITHUB_ACTIONS === 'true') return 'it runs on CI';
  if (platform !== 'win32') return 'the suites run whole only on Windows';
  return '';
}

/** At most this many pull requests are checked together in one batch (R190 V3): a red batch costs a run per member. */
export const BATCH = 3;

/**
 * The parts of the repository `files` touch: the first folder of each file, with what every landing writes left out (the
 * words, the locales the numbering folds into, the fragments), since two batched changes meet there without conflict.
 */
export function areas(files) {
  const words = (f) => WORDS.test(f) || f.startsWith('ui/locales/') || f.startsWith('docs/');
  return new Set(files.filter((f) => !words(f)).map((f) => (f.includes('/') ? f.slice(0, f.indexOf('/')) : f)));
}

/** A change to a workflow lands only from a PC (`--finish`), so it is never batched. */
export const workflow = (files) => files.some((f) => f.startsWith('.github/workflows/'));

/**
 * The queued commit a change with `files` is checked on top of, or none: `queued` are the queue commits that sit on top
 * of today's base, each `{ commit, files, depth, red }` (`files` what it and the ones under it change, `depth` how many
 * pull requests that is, `red` whether its gate failed). The deepest one that touches other parts, changes no workflow,
 * is not red and has room left wins, so the batch grows instead of starting a second one.
 */
export function stackOn(queued, files) {
  if (workflow(files)) return null;
  const mine = areas(files);
  const fits = queued.filter((q) => !q.red && q.depth < BATCH && !workflow(q.files) && ![...areas(q.files)].some((a) => mine.has(a)));
  return fits.sort((a, b) => b.depth - a.depth)[0] ?? null;
}

/**
 * The pull requests a queue commit carries on top of the base, oldest first, read from `git log --reverse
 * --format=%H%x00%s%x00%(trailers:key=Pull-request-head,valueonly)%x1e base..commit`; null when one commit is not a
 * landed pull request's squash (no number in its title or no head line).
 */
export function stacked(log) {
  const out = [];
  for (const record of log.split('\x1e').map((r) => r.trim()).filter(Boolean)) {
    const [commit, subject, oid] = record.split('\0').map((s) => s.trim());
    const pr = /\(#(\d+)\)$/.exec(subject ?? '')?.[1];
    if (!commit || !pr || !oid) return null;
    out.push({ commit, pr, oid });
  }
  return out;
}

const run = (cmd, args, options = {}) => execFileSync(cmd, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true, ...options });
const git = (...args) => run('git', args);

async function land(number, dry, given, args) {
  // What the queue holds before anything is read: the push below replaces only that, so a run that read an older head
  // never overwrites the commit a newer run queued meanwhile.
  const queue = `refs/heads/queue/pr-${number}`;
  const queued = git('ls-remote', 'origin', queue).split(/\s/)[0];
  const pr = JSON.parse(run('gh', ['pr', 'view', number, '--json', 'number,title,body,state,isDraft,baseRefName,isCrossRepository,headRefOid']));
  const refused = refusal(pr);
  if (refused) {
    console.error(`#${pr.number} cannot land: ${refused}.`);
    return 1;
  }
  if (given && given !== pr.headRefOid) {
    console.error(`#${pr.number} was pushed again after ${given.slice(0, 9)} was queued: queue it again, without --head.`);
    return 1;
  }
  const head = given ?? pr.headRefOid;
  git('fetch', '--quiet', 'origin', BASE, given ?? `refs/pull/${pr.number}/head`);
  const base = git('rev-parse', `origin/${BASE}`).trim();
  let tree;
  try {
    tree = git('-c', 'merge.conflictStyle=diff3', 'merge-tree', '--write-tree', '--name-only', base, head).split('\n')[0].trim();
  } catch (error) {
    if (error.status !== 1) throw error;
    const settled = settle(String(error.stdout));
    if (settled.refused) {
      console.error(`#${pr.number} conflicts with ${BASE} in:\n  ${settled.refused.join('\n  ')}`);
      console.error(`Merge origin/${BASE} into its branch, settle them, push, and run this again.`);
      return 1;
    }
    tree = settled.tree;
    console.log(`Lines both sides added at one place in ${conflicts(String(error.stdout).split('\n\n')[0]).join(', ')} are kept, the base's first.`);
  }
  // A change that touches other parts than a pull request queued and not landed yet is checked on top of it, so one run
  // checks both and lands both (R190 V3). The land job's run again (--head) and --alone never batch.
  let parent = base;
  if (!given && !args.includes('--alone') && batches(base)) {
    const on = stackOn(queuedOn(base, pr.number), names(base, tree));
    if (on) {
      try {
        tree = git('merge-tree', '--write-tree', '--name-only', on.commit, head).split('\n')[0].trim();
        parent = on.commit;
        console.log(`Checked together with the ${on.depth} queued under it (${on.commit.slice(0, 9)}): it touches other parts.`);
      } catch (error) {
        if (error.status !== 1) throw error;
        console.log(`It does not apply cleanly on ${on.commit.slice(0, 9)}: it is checked alone.`);
      }
    }
  }
  const squashed = run('git', ['commit-tree', tree, '-p', parent, '-F', '-'], { input: message(pr.title, pr.number, pr.body, head), stdio: ['pipe', 'pipe', 'pipe'] }).trim();
  const commit = numberedCommit(squashed);
  if (!commit) return 1;
  if (dry) {
    console.log(git('show', '--stat', '--format=%B', commit).trim());
    console.log('Dry run: nothing was pushed.');
    return 0;
  }
  // Read only here: CI runs a copy of this file alone (the land job's run again on a new base), which checks nothing.
  const skip = noCheck(process.env, args, process.platform);
  const { STATUS, statusText, toCheck } = skip ? {} : await import('./ci-scope.mjs');
  const suites = skip ? [] : toCheck(git('diff', '--name-only', base, commit).split('\n').map((l) => l.trim()).filter(Boolean));
  const verified = suites.length ? check(commit, suites) : null;
  if (skip) console.log(`No check on this PC (${skip}): CI runs every suite the change needs.`);
  if (verified === false) {
    console.error(`#${pr.number} failed ${suites.join(', ')} on this PC (the lines above): fix it, push, and run this again. Nothing was queued.`);
    return 1;
  }
  if (verified) {
    // GitHub takes a status only on a commit it has: the commit goes to a branch no workflow starts on, the status on it,
    // and only then to the queue, so CI reads the status from its first job.
    try {
      git('push', '--quiet', '--force', 'origin', `${commit}:refs/heads/verify/pr-${pr.number}`);
      const tree = git('rev-parse', `${commit}^{tree}`).trim();
      run('gh', ['api', `repos/{owner}/{repo}/statuses/${commit}`, '-f', 'state=success', '-f', `context=${STATUS}`, '-f', `description=${statusText(tree, suites)}`]);
      console.log(`${suites.join(', ')} passed on this PC: ${STATUS} posted, so CI runs the lints.`);
    } catch (error) {
      console.error(String(error.stderr || error.message).trim());
      console.log(`${STATUS} could not be posted: CI runs every suite.`);
    }
  }
  try {
    git('push', '--quiet', `--force-with-lease=${queue}:${queued}`, 'origin', `${commit}:${queue}`);
  } catch (error) {
    console.error(String(error.stderr || error.message).trim());
    console.error(`queue/pr-${pr.number} was not replaced: when another landing of #${pr.number} queued it meanwhile, that newer one stands.`);
    return 1;
  }
  // A `queue` status on the pull request shows it is queued; the report job of ci.yml turns it red if the queue fails.
  try {
    run('gh', ['api', `repos/{owner}/{repo}/statuses/${head}`, '-f', 'state=pending', '-f', 'context=queue', '-f', `description=Queued as ${commit.slice(0, 9)}; CI lands it once green`]);
  } catch {
    // Only a light on the pull request: the landing goes on without it.
  }
  if (verified) {
    try {
      git('push', '--quiet', 'origin', `:refs/heads/verify/pr-${pr.number}`);
    } catch {
      // The queue branch holds the commit now; a branch left behind costs nothing.
    }
  }
  console.log(`#${pr.number} is queued as ${commit.slice(0, 9)} on queue/pr-${pr.number}. CI checks it, lands it and marks the pull request merged.`);
  return 0;
}

/**
 * Runs each of `suites` whole on `commit`, in the one worktree this PC keeps for landings, with Cargo's own build folder
 * under the main checkout's `target`, so each landing builds only what changed. True when every suite passed, false when
 * one failed, null when it could not run here (no `cargo`, or another landing checks now), so CI runs every suite.
 */
function check(commit, suites) {
  const common = resolve(git('rev-parse', '--path-format=absolute', '--git-common-dir').trim());
  const main = dirname(common);
  const lock = join(common, 'faber-landing.lock');
  if (!take(lock)) {
    console.log('Another landing checks on this PC now: no check here, so CI runs every suite.');
    return null;
  }
  try {
    try {
      run('cargo', ['--version']);
    } catch {
      console.log('No cargo on the PATH here: no check, so CI runs every suite.');
      return null;
    }
    // Beside the main checkout, as the other faber-wt-<name> worktrees, so it builds into target/landing; never under
    // .claude/worktrees, where a lane could have the same name.
    const dir = join(dirname(main), 'faber-wt-landing');
    git('worktree', 'prune');
    if (!existsSync(dir)) {
      git('worktree', 'add', '--quiet', '--detach', dir, commit);
      writeFileSync(join(dir, MARK), 'Kept by scripts/land.mjs for the checks before a landing. Nothing else works here.\n');
    }
    if (!ours(dir)) {
      console.log(`${dir} is not the landing's own worktree: no check here, so CI runs every suite.`);
      return null;
    }
    run('git', ['checkout', '--quiet', '--force', '--detach', commit], { cwd: dir });
    run('git', ['clean', '--quiet', '-fdx', '-e', 'node_modules', '-e', MARK], { cwd: dir });
    if (needsInstall(dir)) {
      const npm = join(dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js');
      execFileSync(process.execPath, [npm, 'ci', '--no-audit', '--no-fund'], { cwd: dir, stdio: 'inherit', windowsHide: true });
      writeFileSync(join(dir, 'node_modules', '.faber-lock'), readFileSync(join(dir, 'package-lock.json')));
    }
    const env = { ...process.env, CARGO_TARGET_DIR: join(main, 'target', 'landing') };
    for (const suite of suites) {
      console.log(`Checking ${suite} on this PC before queueing (cargo t --suite ${suite})...`);
      try {
        execFileSync('cargo', ['t', '--suite', suite], { cwd: dir, env, stdio: 'inherit', windowsHide: true });
      } catch {
        return false;
      }
    }
    // What ran is what is queued: the worktree still holds that very commit, unchanged.
    const head = run('git', ['rev-parse', 'HEAD'], { cwd: dir }).trim();
    const dirty = run('git', ['status', '--porcelain', '--untracked-files=no'], { cwd: dir }).trim();
    if (head !== commit || dirty) {
      console.log('The landing worktree changed while the suites ran: no check, so CI runs every suite.');
      return null;
    }
    return true;
  } finally {
    rmSync(lock, { force: true });
  }
}

/** The file that marks the landing's own worktree. */
const MARK = '.faber-landing';

/** Whether `dir` is the landing's own worktree: registered with git at that very path, detached, and marked. */
function ours(dir) {
  if (!existsSync(join(dir, MARK))) return false;
  let real;
  try {
    real = realpathSync(dir);
  } catch {
    return false;
  }
  const same = (a, b) => (process.platform === 'win32' ? a.toLowerCase() === b.toLowerCase() : a === b);
  const flat = (p) => resolve(p).replace(/[\\/]+$/, '');
  if (!same(flat(real), flat(dir))) return false;
  const entries = git('worktree', 'list', '--porcelain').split(/\n\n+/);
  return entries.some((e) => {
    const path = /^worktree (.+)$/m.exec(e)?.[1];
    return path && same(flat(path), flat(dir)) && /^detached$/m.test(e);
  });
}

/** Whether the worktree's packages are missing or older than its lock file. */
function needsInstall(dir) {
  const stamp = join(dir, 'node_modules', '.faber-lock');
  return !existsSync(stamp) || readFileSync(stamp, 'utf8') !== readFileSync(join(dir, 'package-lock.json'), 'utf8');
}

/**
 * Takes the landing lock: the file appears whole with this process's id (a link to a file written first), so another
 * landing never reads it empty. One left by a process that is gone is moved aside, then taken again; a lock that cannot
 * be read counts as held. Two landings that read the same dead id at once can both get in (Node has no exclusive open);
 * then each checks out before its suites, and the HEAD check after them keeps either from posting a status for a tree
 * it did not test, so the race costs a run, never a false green.
 */
function take(lock) {
  const mine = `${lock}.${process.pid}`;
  writeFileSync(mine, String(process.pid));
  try {
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        linkSync(mine, lock);
        return true;
      } catch {
        let pid = 0;
        try {
          pid = Number(readFileSync(lock, 'utf8').trim());
        } catch {
          return false;
        }
        if (!pid || alive(pid)) return false;
        try {
          renameSync(lock, `${lock}.${pid}.gone`);
          rmSync(`${lock}.${pid}.gone`, { force: true });
        } catch {
          return false;
        }
      }
    }
    return false;
  } finally {
    rmSync(mine, { force: true });
  }
}

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === 'EPERM';
  }
}

/** Whether `base`'s queue jobs land batches: before they do, a batch would land with only its top one marked merged. */
const batches = (base) => git('show', `${base}:.github/workflows/ci.yml`).includes('--land "$GITHUB_SHA"');

/** Deletes queue/pr-<number> only while it still holds `commit`: a newer queueing stands. */
function drop(number, commit) {
  try {
    git('push', '--quiet', `--force-with-lease=refs/heads/queue/pr-${number}:${commit}`, 'origin', `:refs/heads/queue/pr-${number}`);
  } catch {
    // Gone already, or queued again meanwhile.
  }
}

/** The files that differ between two commits or trees. */
const names = (from, to) =>
  git('diff', '--name-only', from, to)
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);

const LOG = '--format=%H%x00%s%x00%(trailers:key=Pull-request-head,valueonly)%x1e';

/** Whether `older` is `newer` or one of its ancestors. */
function ancestor(older, newer) {
  try {
    git('merge-base', '--is-ancestor', older, newer);
    return true;
  } catch {
    return false;
  }
}

/** The commit queue/pr-<number> holds on GitHub now, or ''. */
const queuedAt = (number) => git('ls-remote', 'origin', `refs/heads/queue/pr-${number}`).split(/\s/)[0];

/** How `commit`'s `gate` ended: 'success', 'failure', or '' while it runs or never ran. */
function gate(commit) {
  const ends = run('gh', ['api', `repos/{owner}/{repo}/commits/${commit}/check-runs?check_name=gate`, '--jq', '[.check_runs[].conclusion // ""] | join(" ")']);
  return ends.includes('success') ? 'success' : ends.includes('failure') ? 'failure' : '';
}

/** The other pull requests' queue commits that sit on top of `base` now, as `stackOn` reads them. */
function queuedOn(base, number) {
  const refs = git('ls-remote', 'origin', 'refs/heads/queue/pr-*')
    .split('\n')
    .map((l) => l.trim().split(/\s+/))
    .filter(([sha, ref]) => sha && ref && ref !== `refs/heads/queue/pr-${number}`);
  if (!refs.length) return [];
  git('fetch', '--quiet', 'origin', ...refs.map(([, ref]) => `+${ref}:refs/remotes/origin/${ref.slice('refs/heads/'.length)}`));
  return refs.flatMap(([commit]) => {
    if (!ancestor(base, commit)) return [];
    const batch = stacked(git('log', '--reverse', LOG, `${base}..${commit}`));
    if (!batch?.length) return [];
    return [{ commit, files: names(base, commit), depth: batch.length, red: gate(commit) === 'failure' }];
  });
}

/** Queues #`number` again alone on today's base, from the head `oid` it was squashed from, and starts its run. */
async function alone(number, oid, why) {
  console.log(`${why}: #${number} is squashed again alone on ${BASE} and checked again.`);
  const code = await land(number, false, oid, ['--alone']);
  if (code === 0) run('gh', ['workflow', 'run', 'ci.yml', '--ref', `queue/pr-${number}`]);
  return code;
}

/**
 * The land job's landing of queue/pr-<number> at `sha`, whose gate passed (ci.yml). It carries every pull request
 * batched under it: each branch moves to its own commit, then phase-1-base to `sha` in one push, so GitHub shows each
 * merged. One under it that was queued again, closed, pushed again or whose own gate failed sends this one back alone;
 * so does a base that moved.
 */
async function landQueued(number, sha) {
  git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`, '+refs/heads/main:refs/remotes/origin/main');
  const base = git('rev-parse', `origin/${BASE}`).trim();
  const batch = stacked(git('log', '--reverse', LOG, `${base}..${sha}`));
  const mine = batch?.at(-1);
  if (!mine || mine.pr !== String(number) || mine.commit !== sha) {
    console.error(`${sha.slice(0, 9)} is not #${number}'s squash on top of ${BASE}.`);
    return 1;
  }
  if (!ancestor(base, sha)) return alone(number, mine.oid, `${BASE} moved`);
  const under = batch.slice(0, -1);
  for (const m of under) {
    if (queuedAt(m.pr) !== m.commit || gate(m.commit) === 'failure') return alone(number, mine.oid, `#${m.pr} under it no longer stands as queued`);
  }
  const views = batch.map((m) => ({ ...m, view: JSON.parse(run('gh', ['pr', 'view', m.pr, '--json', 'state,isCrossRepository,baseRefName,headRefName,headRefOid'])) }));
  for (const m of views) {
    if (!refusal({ ...m.view, isDraft: false }) && m.view.headRefOid === m.oid) continue;
    if (m !== views.at(-1)) return alone(number, mine.oid, `#${m.pr} under it was closed, retargeted or pushed again`);
    drop(number, sha);
    console.log(`::warning::#${number} was closed, retargeted or pushed again after it was queued. Queue it again: node scripts/land.mjs ${number}`);
    return 0;
  }
  // The branches first, then the base: GitHub marks a pull request merged only when the base takes the head it already has
  // (one atomic push of both closed #64 instead). A refusal puts every branch moved back.
  const moved = [];
  const back = () => {
    for (const m of moved) {
      try {
        git('push', '--quiet', `--force-with-lease=refs/heads/${m.view.headRefName}:${m.commit}`, 'origin', `${m.oid}:refs/heads/${m.view.headRefName}`);
      } catch (error) {
        // Pushed again meanwhile: that newer head stands. The others are still put back.
        console.error(String(error.stderr || error.message).trim());
      }
    }
  };
  try {
    for (const m of views) {
      git('push', '--quiet', `--force-with-lease=refs/heads/${m.view.headRefName}:${m.oid}`, 'origin', `${m.commit}:refs/heads/${m.view.headRefName}`);
      moved.push(m);
    }
    execFileSync(process.execPath, ['-e', 'setTimeout(() => {}, 5000)'], { windowsHide: true });
    git('push', '--quiet', 'origin', `${sha}:refs/heads/${BASE}`);
  } catch (error) {
    back();
    console.error(String(error.stderr || error.message).trim());
    git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`);
    if (git('rev-parse', `origin/${BASE}`).trim() === base) {
      console.error('::error::The push was refused although phase-1-base did not move; the push error is above.');
      return 1;
    }
    return alone(number, mine.oid, `${BASE} moved`);
  }
  // Green on top of the whole phase-1-base, so main follows at once (R127). Pushes made with this token start no
  // workflow, so main's warm run (R165) and the preview release (R130, R156) are started by hand.
  if (ancestor('origin/main', sha)) {
    git('push', '--quiet', 'origin', `${sha}:refs/heads/main`);
    run('gh', ['workflow', 'run', 'ci.yml', '--ref', 'main', '-f', 'warm=true']);
  }
  for (const m of views) drop(m.pr, m.commit);
  run('gh', ['workflow', 'run', 'auto-release.yml', '--ref', BASE, '-f', `sha=${sha}`]);
  console.log(`Landed ${sha}${under.length ? `, with ${under.map((m) => `#${m.pr}`).join(', ')} under it` : ''}.`);
  return 0;
}

/**
 * The report job's reading of a red queue/pr-<number> at `sha` (ci.yml): batched on others that have not landed, the red
 * may be theirs, so it is queued again alone and the batch is split in two (bisection); each side then answers for itself.
 * True when it went back alone, false when the red is its own.
 */
async function split(number, sha) {
  git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`);
  const base = git('rev-parse', `origin/${BASE}`).trim();
  // Landed under a batch meanwhile, or queued again since: nothing to blame here.
  if (ancestor(sha, base) || queuedAt(number) !== sha) return true;
  const batch = stacked(git('log', '--reverse', LOG, `${base}..${sha}`));
  if (!batch || batch.length < 2 || batch.at(-1).pr !== String(number)) return false;
  return (await alone(number, batch.at(-1).oid, `The batch under #${number} was red`)) === 0;
}

/** `commit` with its fragments folded in and its placeholders numbered, made in a throwaway worktree; none when it cannot. */
function numberedCommit(commit) {
  const dir = mkdtempSync(join(tmpdir(), 'faber-land-'));
  try {
    git('worktree', 'add', '--quiet', '--detach', dir, commit);
    try {
      console.log(run('node', ['scripts/numbers.mjs', 'assign', '--amend'], { cwd: dir }).trim());
    } catch (error) {
      console.error(String(error.stderr || error.stdout || error.message).trim());
      return '';
    }
    return run('git', ['rev-parse', 'HEAD'], { cwd: dir }).trim();
  } finally {
    git('worktree', 'remove', '--force', dir);
    rmSync(dir, { recursive: true, force: true });
  }
}

/**
 * Lands from this PC a queued commit that CI checked but could not push itself, because it changes a workflow file
 * (GitHub refuses that to the workflow's token): only when its `gate` passed, it still sits on top of phase-1-base and
 * the pull request is still at the head that was squashed. The same atomic push as the land job of ci.yml.
 */
function finish(number) {
  const queue = `queue/pr-${number}`;
  git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`, `+refs/heads/${queue}:refs/remotes/origin/${queue}`, '+refs/heads/main:refs/remotes/origin/main');
  const sha = git('rev-parse', `origin/${queue}`).trim();
  const gate = run('gh', ['api', `repos/{owner}/{repo}/commits/${sha}/check-runs?check_name=gate`, '--jq', '[.check_runs[] | select(.conclusion == "success")] | length']).trim();
  if (gate === '0') {
    console.error(`The gate of ${sha.slice(0, 9)} has not passed (yet): nothing lands.`);
    return 1;
  }
  try {
    git('merge-base', '--is-ancestor', `origin/${BASE}`, sha);
  } catch {
    console.error(`${BASE} moved after #${number} was queued: queue it again with node scripts/land.mjs ${number}.`);
    return 1;
  }
  const oid = git('log', '-1', '--format=%(trailers:key=Pull-request-head,valueonly)', sha).trim();
  const pr = JSON.parse(run('gh', ['pr', 'view', number, '--json', 'state,isCrossRepository,baseRefName,headRefName,headRefOid']));
  if (refusal({ ...pr, isDraft: false }) || pr.headRefOid !== oid) {
    console.error(`#${number} was closed, retargeted or pushed again after it was queued: queue it again.`);
    return 1;
  }
  // The branch first, then the base, as the land job does: GitHub marks the pull request merged only when the base takes
  // the head it already has. A base that refuses puts the branch back.
  const branch = `refs/heads/${pr.headRefName}`;
  git('push', '--quiet', `--force-with-lease=${branch}:${oid}`, 'origin', `${sha}:${branch}`);
  execFileSync(process.execPath, ['-e', 'setTimeout(() => {}, 5000)'], { windowsHide: true });
  try {
    git('push', '--quiet', 'origin', `${sha}:refs/heads/${BASE}`);
  } catch (error) {
    git('push', '--quiet', `--force-with-lease=${branch}:${sha}`, 'origin', `${oid}:${branch}`);
    console.error(String(error.stderr || error.message).trim());
    console.error(`${BASE} refused it, so #${number}'s branch is back where it was: queue it again.`);
    return 1;
  }
  try {
    git('merge-base', '--is-ancestor', 'origin/main', sha);
    git('push', '--quiet', 'origin', `${sha}:refs/heads/main`);
  } catch {
    // main moved on its own: it follows the next green commit (R127).
  }
  git('push', '--quiet', 'origin', `:refs/heads/${queue}`);
  console.log(`#${number} landed as ${sha.slice(0, 9)} on ${BASE}.`);
  return 0;
}

// Real paths on both sides: a temporary folder may be a link (macOS's /var is /private/var), and Node gives the module its real path.
if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(import.meta.filename)) {
  const args = process.argv.slice(2);
  const number = args.find((a) => /^\d+$/.test(a));
  if (!number) {
    console.error('usage: node scripts/land.mjs <pull request> [--dry-run] [--no-verify] [--alone] [--head <commit>] | <pull request> --finish');
    process.exitCode = 2;
  } else if (args.includes('--finish')) {
    process.exitCode = finish(number);
  } else if (args.includes('--land')) {
    process.exitCode = await landQueued(number, args[args.indexOf('--land') + 1]);
  } else if (args.includes('--split')) {
    // Exit 3: queued again alone, so the report job blames nobody yet.
    process.exitCode = (await split(number, args[args.indexOf('--split') + 1])) ? 3 : 0;
  } else {
    const at = args.indexOf('--head');
    process.exitCode = await land(number, args.includes('--dry-run'), at >= 0 ? args[at + 1] : undefined, args);
  }
}
