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
// the batch lands as checked if the base moved only by words or independent files; otherwise it is squashed and numbered
// again on the new base. Independent files get a full base check before main follows. A conflict stops with its files.
// --dry-run shows the commit it would queue and pushes nothing.
//
// Trains (R-next): once the base's ci.yml runs them, a queued pull request waits for the next train, which `--train`
// builds on CI: the oldest queued ones, each squashed on the one before it, checked by one run and landed together by the
// land job (`--land`); a red train sends its first half again (`--split`). Before that, batches (R190 V3): a pull request
// that touches other parts than one queued is squashed on top of it (at most `BATCH` deep). `--alone` never batches.
//
// Before it queues, it runs on this PC the suites CI would run for the change (`cargo t --suite <name>`, each whole) on
// the very commit it queues, in one worktree kept for that (R190 V2). When they pass, it posts the commit status
// `faber/verify` with that tree and those suites, and CI leaves them out; the lints still run there. With --no-verify,
// on CI (the land job's run again on a new base), with no `cargo` here, or while another landing checks on this PC, it
// queues without one and CI runs every suite as before. A failure here queues nothing.
//
// `--sweep` (the land job of ci.yml, after each landing) runs again the landing of the oldest queue run whose gate passed
// but whose landing GitHub cancelled while it waited for another, so a green queue run is never lost.

import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { existsSync, linkSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const BASE = 'phase-1-base';

/** Keep an empty-output proof failure diagnosable without copying its potentially large output. */
export function localProofFailure(suite, error) {
  const reason = Number.isInteger(error.status) ? `exit ${error.status}`
    : error.signal ? `signal ${error.signal}`
      : error.code ? `start error ${error.code}` : 'no exit status';
  return `Local ${suite} proof failed (${reason}). Nothing was queued.`;
}

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
    const blocks = sides.map((lines) => `\n${lines.join('\n')}\n`);
    if (blocks[0].includes(blocks[2])) out.push(...sides[0]);
    else if (blocks[2].includes(blocks[0])) out.push(...sides[2]);
    else out.push(...sides[0], ...sides[2]);
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

/**
 * Whether a green batch may land without a new run on a base that moved by the files `moved` since it was checked, its
 * tree then differing from the one checked by `differs`: words only on both sides (`isWords`, the rule CI skips every
 * suite by), and none of them where numbers are taken, since the batch was numbered on the old base.
 */
export function wordsOnly(moved, differs, isWords) {
  const numbered = (f) => f === 'docs/decisions.md' || f.startsWith('docs/next/');
  return moved.length > 0 && moved.every((f) => isWords(f) && !numbered(f)) && differs.every(isWords);
}

/** Whether the checked batch can land on a base that moved, and why. */
export function landsAsChecked(moved, changed, differs, isWords, { ownWorkflow = false } = {}) {
  // A workflow change of its own lands only from a PC (`--finish`, `ownWorkflow`); one on the base changed what checked it.
  if (workflow(moved) || (!ownWorkflow && workflow(changed))) return null;
  if (wordsOnly(moved, differs, isWords)) return 'words';
  const numbered = (f) => f === 'docs/decisions.md' || f.startsWith('docs/next/') || f.startsWith('ui/locales/');
  const files = new Set(moved);
  if (moved.length && !changed.some((f) => files.has(f)) && differs.every((f) => files.has(f)) &&
      !(moved.some(numbered) && changed.some(numbered))) return 'disjoint';
  return null;
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
  const fits = queued.filter((q) => q.eligible !== false && !q.red && q.depth < BATCH && !workflow(q.files) && ![...areas(q.files)].some((a) => mine.has(a)));
  return fits.sort((a, b) => b.depth - a.depth)[0] ?? null;
}

/** A stacked queue commit is safe to build on only while every pull request still matches its queued squash. */
export function eligibleStack(batch, pullRequests, queueHeads, redCommits) {
  return batch.every(({ commit, pr, oid }) => {
    const view = pullRequests[pr];
    return view?.state === 'OPEN' && !refusal(view) && view.headRefOid === oid && queueHeads[pr] === commit && !redCommits.has(commit);
  });
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

async function land(number, dry, given, args, opts = {}) {
  // What the queue holds before anything is read: the push below replaces only that, so a run that read an older head
  // never overwrites the commit a newer run queued meanwhile.
  const queue = `refs/heads/queue/pr-${number}`;
  const queued = git('ls-remote', 'origin', queue).split(/\s/)[0];
  const pr = JSON.parse(run('gh', ['pr', 'view', number, '--json', 'number,title,body,state,isDraft,baseRefName,isCrossRepository,headRefOid,headRefName']));
  const refused = refusal(pr);
  if (refused) {
    console.error(`#${pr.number} cannot land: ${refused}.`);
    return 1;
  }
  // A lane lands what it queued before something new (docs/dev/workflow.md#owed-work); the runs on CI record nothing.
  // Loaded only here: CI runs a copy of this file alone, without the files beside it.
  const lane = !given && process.env.GITHUB_ACTIONS !== 'true' ? await import('./owed.mjs') : null;
  // Any owed pull request may be queued again: each one's fix is what the guard waits for.
  const owed = lane ? lane.check() : [];
  if (owed.length && !owed.some((other) => other.number === pr.number)) {
    console.error(lane.message(owed));
    console.error(`#${pr.number} was not queued. FABER_IGNORE_OWED=1 queues it anyway, for work those wait on.`);
    return 1;
  }
  if (given && given !== pr.headRefOid) {
    console.error(`#${pr.number} was pushed again after ${given.slice(0, 9)} was queued: queue it again, without --head.`);
    return 1;
  }
  const head = given ?? pr.headRefOid;
  git('fetch', '--quiet', 'origin', BASE, given ?? `refs/pull/${pr.number}/head`);
  // A train squashes each rider on the one before it (`opts.onto`), not on the base.
  const base = opts.onto ?? git('rev-parse', `origin/${BASE}`).trim();
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
  lane?.remember(pr.number, pr.headRefName);
  opts.commit = commit;
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
      } catch (error) {
        console.error(localProofFailure(suite, error));
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
const batches = (base) => {
  const ci = git('show', `${base}:.github/workflows/ci.yml`);
  // Once trains run, the train squashes every rider again on the one before it: stacking here would only be undone.
  return ci.includes('--land "$GITHUB_SHA"') && !ci.includes('--train');
};

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
  const queueHeads = Object.fromEntries(refs.map(([commit, ref]) => [ref.slice('refs/heads/queue/pr-'.length), commit]));
  const pullRequests = new Map();
  const gates = new Map();
  const viewsFor = (pr) => {
    if (!pullRequests.has(pr)) {
      try {
        pullRequests.set(pr, JSON.parse(run('gh', ['pr', 'view', pr, '--json', 'state,isDraft,baseRefName,isCrossRepository,headRefOid'])));
      } catch {
        pullRequests.set(pr, null);
      }
    }
    return pullRequests.get(pr);
  };
  const redCommits = new Set();
  const failedGate = (commit) => {
    if (!gates.has(commit)) {
      try {
        gates.set(commit, gate(commit) === 'failure');
      } catch {
        gates.set(commit, true);
      }
    }
    return gates.get(commit);
  };
  return refs.flatMap(([commit]) => {
    if (!ancestor(base, commit)) return [];
    const batch = stacked(git('log', '--reverse', LOG, `${base}..${commit}`));
    if (!batch?.length) return [];
    const views = Object.fromEntries(batch.map((m) => [m.pr, viewsFor(m.pr)]));
    const queued = Object.fromEntries(batch.map((m) => [m.pr, queueHeads[m.pr] ?? '']));
    for (const m of batch) if (failedGate(m.commit)) redCommits.add(m.commit);
    return [{ commit, files: names(base, commit), depth: batch.length, red: redCommits.has(commit), eligible: eligibleStack(batch, views, queued, redCommits) }];
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
 * The green batch `sha` put on the moved `base`: its new top, each member's commit and why it lands as checked.
 * Null when the trees do not qualify, a member does not apply cleanly, or a check fails on the new tree.
 */
export async function onMoved(base, sha, batch, rule = {}) {
  // The checked-out queued commit's own rules, as CI's: the base's land.mjs runs from a temporary folder.
  const { isWords } = await import(pathToFileURL(resolve('scripts/ci-scope.mjs')).href);
  // Both sides of a rename count: a code file renamed into a folder of words is not words.
  const plain = (from, to) => git('diff', '--name-only', '--no-renames', from, to).split('\n').filter(Boolean);
  const mergeBase = git('merge-base', base, sha).trim();
  const moved = plain(mergeBase, base);
  const changed = plain(mergeBase, sha);
  if (!landsAsChecked(moved, changed, [], isWords, rule)) return null;
  const dir = mkdtempSync(join(tmpdir(), 'faber-land-'));
  try {
    git('worktree', 'add', '--quiet', '--detach', dir, base);
    git('-C', dir, 'cherry-pick', '--allow-empty', ...batch.map((m) => m.commit));
    const landing = git('-C', dir, 'rev-parse', 'HEAD').trim();
    const kind = landsAsChecked(moved, changed, plain(sha, landing), isWords, rule);
    if (!kind) return null;
    run(process.execPath, ['scripts/docs.mjs'], { cwd: dir });
    run(process.execPath, ['scripts/numbers.mjs', 'check'], { cwd: dir });
    const commits = git('-C', dir, 'rev-list', '--reverse', `${base}..${landing}`).split('\n').filter(Boolean);
    if (commits.length !== batch.length) return null;
    console.log(`${BASE} moved by ${kind} (${moved.join(', ')}): the green batch lands on it as checked.`);
    return { landing, commits, kind };
  } catch (error) {
    const lines = String(error.stderr || error.message).trim().split('\n');
    console.log(`${BASE} moved, but the batch does not land on it as checked: ${lines.find((l) => /error|conflict/i.test(l)) ?? lines.at(-1)}`);
    return null;
  } finally {
    try {
      git('worktree', 'remove', '--force', dir);
    } catch {
      rmSync(dir, { recursive: true, force: true });
    }
  }
}

/**
 * The land job's landing of queue/pr-<number> at `sha`, whose gate passed (ci.yml). It carries every pull request
 * batched under it: each branch moves to its own commit, then phase-1-base to `sha` in one push, so GitHub shows each
 * merged. One under it that was queued again, closed, pushed again or whose own gate failed sends this one back alone;
 * so does a base that moved.
 */
async function landQueued(number, sha, self) {
  git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`, '+refs/heads/main:refs/remotes/origin/main');
  const base = git('rev-parse', `origin/${BASE}`).trim();
  const batch = stacked(git('log', '--reverse', LOG, `${base}..${sha}`));
  const mine = batch?.at(-1);
  if (!mine || mine.pr !== String(number) || mine.commit !== sha) {
    console.error(`${sha.slice(0, 9)} is not #${number}'s squash on top of ${BASE}.`);
    return 1;
  }
  // A train that cannot land as checked never goes back alone: the next train takes whoever still waits.
  const inTrain = trainAt() === sha;
  const resend = (why) => (inTrain ? next(sha, self, why) : alone(number, mine.oid, why));
  // A base that moved by words or independent files can take the green batch as checked.
  const movedBase = ancestor(base, sha) ? null : await onMoved(base, sha, batch);
  if (!ancestor(base, sha) && !movedBase) return resend(`${BASE} moved`);
  const top = movedBase?.landing ?? sha;
  const under = batch.slice(0, -1);
  for (const m of under) {
    if (queuedAt(m.pr) !== m.commit || gate(m.commit) === 'failure') return resend(`#${m.pr} under it no longer stands as queued`);
  }
  const views = batch.map((m, i) => ({ ...m, landed: movedBase?.commits[i] ?? m.commit, view: JSON.parse(run('gh', ['pr', 'view', m.pr, '--json', 'state,isCrossRepository,baseRefName,headRefName,headRefOid'])) }));
  for (const m of views) {
    if (!refusal({ ...m.view, isDraft: false }) && m.view.headRefOid === m.oid) continue;
    if (m !== views.at(-1)) return resend(`#${m.pr} under it was closed, retargeted or pushed again`);
    drop(number, sha);
    console.log(`::warning::#${number} was closed, retargeted or pushed again after it was queued. Queue it again: node scripts/land.mjs ${number}`);
    return inTrain ? next(sha, self, `#${number} left the train`) : 0;
  }
  // The branches first, then the base: GitHub marks a pull request merged only when the base takes the head it already has
  // (one atomic push of both closed #64 instead). A refusal puts every branch moved back.
  const moved = [];
  const back = () => {
    for (const m of moved) {
      try {
        git('push', '--quiet', `--force-with-lease=refs/heads/${m.view.headRefName}:${m.landed}`, 'origin', `${m.oid}:refs/heads/${m.view.headRefName}`);
      } catch (error) {
        // Pushed again meanwhile: that newer head stands. The others are still put back.
        console.error(String(error.stderr || error.message).trim());
      }
    }
  };
  try {
    for (const m of views) {
      git('push', '--quiet', `--force-with-lease=refs/heads/${m.view.headRefName}:${m.oid}`, 'origin', `${m.landed}:refs/heads/${m.view.headRefName}`);
      moved.push(m);
    }
    execFileSync(process.execPath, ['-e', 'setTimeout(() => {}, 5000)'], { windowsHide: true });
    git('push', '--quiet', 'origin', `${top}:refs/heads/${BASE}`);
  } catch (error) {
    back();
    console.error(String(error.stderr || error.message).trim());
    git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`);
    if (git('rev-parse', `origin/${BASE}`).trim() === base) {
      console.error('::error::The push was refused although phase-1-base did not move; the push error is above.');
      return 1;
    }
    return resend(`${BASE} moved`);
  }
  // Independent changes need a full base check first, and so does every landing while main waits behind the base for
  // one: a later batch's own check may not cover the whole tree. Otherwise main follows at once and starts its warm run
  // (R165).
  const checkBase = movedBase?.kind === 'disjoint' || !ancestor(base, 'origin/main');
  if (checkBase) {
    run('gh', ['workflow', 'run', 'ci.yml', '--ref', BASE]);
    console.log('main follows once the phase-1-base check is green.');
  } else {
    if (ancestor('origin/main', top)) {
      git('push', '--quiet', 'origin', `${top}:refs/heads/main`);
      run('gh', ['workflow', 'run', 'ci.yml', '--ref', 'main', '-f', 'warm=true']);
    }
    run('gh', ['workflow', 'run', 'auto-release.yml', '--ref', BASE, '-f', `sha=${top}`]);
  }
  for (const m of views) drop(m.pr, m.commit);
  // The board follows the landing (docs/dev/workflow.md#board); one that fails to publish never fails the landing.
  try {
    run('node', ['scripts/tasks.mjs', 'board', '--push']);
  } catch (e) {
    console.log(`The board was not updated: ${e.message.split('\n')[0]}`);
  }
  console.log(`Landed ${top}${under.length ? `, with ${under.map((m) => `#${m.pr}`).join(', ')} under it` : ''}.`);
  return inTrain ? next(sha, self, 'The train landed') : 0;
}

/**
 * The report job's reading of a red queue/pr-<number> at `sha` (ci.yml): batched on others that have not landed, the red
 * may be theirs, so it is queued again alone and the batch is split in two (bisection); each side then answers for itself.
 * True when it went back alone, false when the red is its own.
 */
async function split(number, sha, self) {
  git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`);
  const base = git('rev-parse', `origin/${BASE}`).trim();
  // Landed under a batch meanwhile, or queued again since: nothing to blame here.
  if (ancestor(sha, base) || queuedAt(number) !== sha) return true;
  const batch = stacked(git('log', '--reverse', LOG, `${base}..${sha}`));
  if (!batch || batch.length < 2 || batch.at(-1).pr !== String(number)) return false;
  // A red train rides again with its first half only; the rest wait for the train after it, and a red rider alone is
  // the culprit (bisection).
  if (trainAt() === sha) {
    console.log(`The train ${sha.slice(0, 9)} was red: its first half rides again alone, and its second half next.`);
    const prs = batch.map((m) => m.pr);
    const first = half(prs);
    return (await train({ after: sha, self, only: first, later: prs.slice(first.length) })) === 0;
  }
  return (await alone(number, batch.at(-1).oid, `The batch under #${number} was red`)) === 0;
}

/**
 * Trains (R-next). The queue checks one train at a time: every pull request queued meanwhile waits for the next, squashed
 * on the one before it, so one run checks them all and the land job lands them all. Before trains, each queued commit had
 * its own run; they shared GitHub's twenty runners, a landing moved the base under all the others, and each of those was
 * squashed and checked again from the start (2026-11-01: of 38 queue runs, 1 green, 6 red and 31 cancelled).
 * The branch `train` holds the top of the train that runs; the land and report jobs send the next one when theirs ends,
 * as does every queueing while none runs, and queue.yml every twenty minutes, for a run that stopped without either.
 */
// Three: on 2026-11-01 about four queued pull requests in ten arrived broken, and trains of six were red for six
// hours, since a train lands only when every rider is green. Three keeps a whole train likely to pass; raise it once
// fewer arrive broken.
export const TRAIN = 3;
const LOCK = 'refs/heads/train';

/** The commit the train that runs is checked on (the `train` branch), or ''. */
const trainAt = () => git('ls-remote', 'origin', LOCK).split(/\s/)[0] ?? '';

/**
 * The riders of the next train, oldest queueing first and at most `TRAIN`: queue entries `{ pr, oid, files, time, red,
 * view }` whose pull request still stands at the head that was squashed, changes no workflow (which lands only from a
 * PC) and was not red alone since it was queued; with `only`, those pull requests alone.
 */
export function riders(entries, only) {
  return entries
    .filter((e) => e.oid && !workflow(e.files) && !e.red && e.view && !refusal(e.view) && e.view.headRefOid === e.oid)
    .filter((e) => !only || only.includes(e.pr))
    .sort((a, b) => a.time - b.time || Number(a.pr) - Number(b.pr))
    .slice(0, TRAIN);
}

/** The pull requests of a red train that ride again: the first half, at least one. */
export const half = (prs) => prs.slice(0, Math.max(1, Math.ceil(prs.length / 2)));

/**
 * The halves of red trains still to ride, first group next: a commit on the base whose message has one `Then:` line per
 * group. Without it the second half of a red train went back among the newcomers, and a red rider there turned each
 * train after it red until it happened to ride first (2026-11-01: #569 rode red trains for two hours). Only a train
 * builder, holding the lock, writes it.
 */
const THEN = 'refs/heads/train-then';

/** The groups a `train-then` message names, in order. */
export const parseThen = (message) =>
  message
    .split('\n')
    .filter((l) => l.startsWith('Then: '))
    .map((l) => l.slice('Then: '.length).trim().split(/\s+/).filter(Boolean))
    .filter((g) => g.length);

/** The message that names `groups`. */
export const thenMessage = (groups) =>
  ['Halves of red trains that wait to ride, the first next (R497).', '', ...groups.map((g) => `Then: ${g.join(' ')}`)].join('\n');

/**
 * Who rides next and which groups still wait: `only` (a red train's first half) with `later` (its second half) put in
 * front of the waiting `groups`, else the first waiting group that still has a rider among `entries`. Null rides
 * everyone, once no group is left.
 */
export function pickRiders(entries, groups, only, later) {
  const queue = only ? [only, ...(later?.length ? [later] : []), ...groups] : [...groups];
  while (queue.length) {
    const group = queue.shift();
    if (riders(entries, group).length) return { only: group, groups: queue };
  }
  return { only: null, groups: [] };
}

function readThen() {
  const held = git('ls-remote', 'origin', THEN).split(/\s/)[0] ?? '';
  if (!held) return { held, groups: [] };
  git('fetch', '--quiet', 'origin', THEN);
  return { held, groups: parseThen(git('log', '-1', '--format=%B', held)) };
}

/**
 * The push arguments that leave `train-then` naming `groups` (its lease, then its refspec), none when it already names
 * nothing. They go in the same atomic push as the lock, so a builder that stops halfway loses no group.
 */
function thenPush(base, held, groups) {
  if (!groups.length && !held) return [[], []];
  const target = groups.length
    ? run('git', ['commit-tree', `${base}^{tree}`, '-p', base, '-m', thenMessage(groups)], { stdio: ['ignore', 'pipe', 'pipe'] }).trim()
    : '';
  return [[`--force-with-lease=${THEN}:${held}`], [`${target}:${THEN}`]];
}

/**
 * Whether the train at `sha` still runs: a run of ci.yml on that commit that has not ended, other than `self` (the job
 * asking), or none yet on a train sent less than ten minutes ago, which GitHub may not list yet.
 */
export function running(runs, self, age) {
  if (runs.some((r) => r.status !== 'completed' && String(r.id) !== String(self))) return true;
  return !runs.length && age < 600;
}

/** The queued pull requests, read from queue/pr-* on GitHub; one already on `base` is deleted there. */
function waiting(base) {
  const refs = git('ls-remote', 'origin', 'refs/heads/queue/pr-*')
    .split('\n')
    .map((l) => l.trim().split(/\s+/))
    .filter(([sha, ref]) => sha && ref);
  if (!refs.length) return [];
  git('fetch', '--quiet', 'origin', ...refs.map(([, ref]) => `+${ref}:refs/remotes/origin/${ref.slice('refs/heads/'.length)}`));
  return refs.flatMap(([sha, ref]) => {
    const pr = ref.slice('refs/heads/queue/pr-'.length);
    if (ancestor(sha, base)) {
      drop(pr, sha);
      return [];
    }
    const oid = git('log', '-1', '--format=%(trailers:key=Pull-request-head,valueonly)', sha).trim();
    let view = null;
    try {
      view = JSON.parse(run('gh', ['pr', 'view', pr, '--json', 'state,isDraft,baseRefName,isCrossRepository,headRefOid']));
    } catch {
      // Unreadable: it waits for a train that can read it.
    }
    let red = false;
    try {
      red = oid !== '' && redAt(JSON.parse(run('gh', ['api', `repos/{owner}/{repo}/commits/${oid}/statuses?per_page=100`, '--jq', '[.[] | select(.context == "queue") | {state, description}]'])), sha);
    } catch {
      red = true;
    }
    // Both sides of a rename: a workflow moved out of .github/workflows still changes one.
    const files = git('diff', '--name-only', '--no-renames', `${sha}^`, sha)
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean);
    return [{ pr, oid, commit: sha, files, time: Number(git('log', '-1', '--format=%ct', sha).trim()), red, view }];
  });
}

/**
 * Whether the queueing at `commit` was red: the newest `queue` status on its head (`statuses`, newest first) failed and
 * names that very queue commit. A late report of an older queueing, or a newer pending one, leaves it free to ride.
 */
export function redAt(statuses, commit) {
  const newest = statuses[0];
  return newest?.state === 'failure' && (newest.description ?? '').includes(commit.slice(0, 9));
}

/** Turns #`number`'s queueing red with `why`, on its head and in a comment, and deletes its queue branch. */
function blame(number, oid, commit, why) {
  const at = process.env.GITHUB_RUN_ID ? `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}` : '';
  try {
    run('gh', ['api', `repos/{owner}/{repo}/statuses/${oid}`, '-f', 'state=failure', '-f', 'context=queue', ...(at ? ['-f', `target_url=${at}`] : []), '-f', `description=Queued as ${commit.slice(0, 9)}, but ${why}. Fix, push, then: node scripts/land.mjs ${number}`]);
    run('gh', ['pr', 'comment', number, '--body', `Queued, but ${why}${at ? `: ${at}` : ''}. Push the fix, then queue it again with \`node scripts/land.mjs ${number}\`.`]);
  } catch (error) {
    console.error(String(error.stderr || error.message).trim());
  }
  drop(number, commit);
}

/** The land or report job's end of the train at `sha`: the next one leaves with whoever still waits. */
async function next(sha, self, why) {
  console.log(`${why}: the next train leaves with whoever waits.`);
  return train({ after: sha, self });
}

/**
 * Sends the next train when none runs: `after` is the train that just ended (the `train` branch may still name it), `self`
 * the run asking, `only` the riders a red train sends again. Each rider is squashed and numbered again on the one before
 * it from the head that was queued, its queue branch moved there, and one run of ci.yml checks the top one.
 */
async function train({ after = '', self = '', only = null, later = [] } = {}) {
  git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`);
  const base = git('rev-parse', `origin/${BASE}`).trim();
  const held = trainAt();
  if (held && held !== after) {
    git('fetch', '--quiet', 'origin', LOCK);
    const runs = JSON.parse(run('gh', ['api', `repos/{owner}/{repo}/actions/workflows/ci.yml/runs?head_sha=${held}&per_page=50`, '--jq', '[.workflow_runs[] | {id, status}]']));
    if (running(runs, self, Date.now() / 1000 - Number(git('log', '-1', '--format=%ct', held).trim()))) {
      console.log(`The train ${held.slice(0, 9)} is being checked: whoever is queued waits for the next.`);
      return 0;
    }
    console.log(`The train ${held.slice(0, 9)} stopped without landing or failing: the next one leaves.`);
  }
  // The lock before any queue branch moves, so two builders never interleave: a fresh commit on the base, which counts as
  // a train just sent for ten minutes, the builder's own time. Its own nonce: two builders in the same second would
  // make the same commit, and a push of a commit the branch already holds checks no lease.
  const lock = run('git', ['commit-tree', `${base}^{tree}`, '-p', base, '-m', `A train is being built (${randomUUID()})`], { stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  try {
    git('push', '--quiet', `--force-with-lease=${LOCK}:${held}`, 'origin', `${lock}:${LOCK}`);
  } catch {
    console.log('Another train is being built: whoever is queued waits for it.');
    return 0;
  }
  const entries = waiting(base);
  const then = readThen();
  const pick = pickRiders(entries, then.groups, only, later);
  const chosen = riders(entries, pick.only);
  let parent = base;
  const members = [];
  const behind = [];
  for (const e of chosen) {
    const opts = { onto: parent };
    if ((await land(e.pr, false, e.oid, ['--alone'], opts)) === 0 && opts.commit) {
      parent = opts.commit;
      members.push(e.pr);
    } else if (parent === base) {
      // Not even on the base alone: only its author can settle it.
      blame(e.pr, e.oid, e.commit, `it does not squash on ${BASE}`);
    } else {
      console.log(`#${e.pr} does not go on top of the riders before it: it waits for the next train.`);
      behind.push(e.pr);
    }
  }
  // A group's riders that did not fit ride next, still before any newcomer.
  const groups = pick.only && behind.length ? [behind, ...pick.groups] : pick.groups;
  const [lease, refspec] = thenPush(base, then.held, groups);
  if (!members.length) {
    try {
      git('push', '--quiet', '--atomic', `--force-with-lease=${LOCK}:${lock}`, ...lease, 'origin', `:${LOCK}`, ...refspec);
    } catch {
      // Taken over after ten minutes: that train stands, and so does the list it found.
    }
    console.log('Nobody waits for a train.');
    return 0;
  }
  try {
    git('push', '--quiet', '--atomic', `--force-with-lease=${LOCK}:${lock}`, ...lease, 'origin', `${parent}:${LOCK}`, ...refspec);
  } catch {
    console.log('Another train left meanwhile: these wait for the one after it.');
    return 0;
  }
  if (groups.length) console.log(`Waiting to ride after this train: ${groups.map((g) => g.map((n) => `#${n}`).join(' ')).join(', then ')}.`);
  // Pushes made with this token start no workflow, so the train's run is started by hand.
  run('gh', ['workflow', 'run', 'ci.yml', '--ref', `queue/pr-${members.at(-1)}`]);
  console.log(`The train ${parent.slice(0, 9)} leaves with ${members.map((n) => `#${n}`).join(', ')}: one run checks them together.`);
  return 0;
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
 * The queue runs whose landing may have been dropped while it waited, oldest first. GitHub keeps one waiting job in the
 * `land` group of ci.yml and cancels the older one, so a run whose gate passed can end cancelled with nothing landed.
 * `runs` are ci.yml runs ({ databaseId, headBranch, headSha, createdAt, status, conclusion }); only the newest of each
 * queue branch counts, and only while the branch still points at it (`heads`, branch to commit): a
 * branch pushed again has its own run, and a landed one is deleted.
 */
export function dropped(runs, heads) {
  const newest = new Map();
  for (const r of runs) {
    const seen = newest.get(r.headBranch);
    if (r.headBranch.startsWith('queue/') && (!seen || Date.parse(r.createdAt) > Date.parse(seen.createdAt))) newest.set(r.headBranch, r);
  }
  return [...newest.values()]
    .filter((r) => r.status === 'completed' && r.conclusion === 'cancelled' && heads.get(r.headBranch) === r.headSha)
    .sort((a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt));
}

/** Whether a run's jobs ({ name, conclusion }) say its gate passed and its landing was cancelled. */
export const landCancelled = (jobs) =>
  jobs.some((j) => j.name === 'gate' && j.conclusion === 'success') && jobs.some((j) => j.name === 'land' && j.conclusion === 'cancelled');

/**
 * Runs again the landing of the oldest queue run dropped while it waited (`dropped`), and only that one: a second would
 * wait in the same group and cancel it again. Its own landing sweeps again when it ends, so they land one after another.
 */
function sweep() {
  const heads = new Map(
    git('ls-remote', 'origin', 'refs/heads/queue/*')
      .split('\n')
      .filter(Boolean)
      .map((l) => l.split('\t'))
      .map(([sha, ref]) => [ref.slice('refs/heads/'.length), sha]),
  );
  // Cancelled runs only, so the window spans days of landings: the head check above drops what moved since.
  const runs = JSON.parse(run('gh', ['run', 'list', '--workflow', 'ci.yml', '--status', 'cancelled', '--limit', '100', '--json', 'databaseId,headBranch,headSha,createdAt,status,conclusion']));
  for (const r of dropped(runs, heads)) {
    const { jobs } = JSON.parse(run('gh', ['run', 'view', String(r.databaseId), '--json', 'jobs']));
    if (!landCancelled(jobs)) continue;
    try {
      run('gh', ['run', 'rerun', String(r.databaseId), '--failed']);
    } catch (error) {
      // GitHub may refuse one (too many attempts, a run too old): the next one still lands.
      console.error(`Run ${r.databaseId} of ${r.headBranch} could not run again: ${String(error.stderr || error.message).trim()}`);
      continue;
    }
    console.log(`The landing of ${r.headBranch} was dropped while it waited: run ${r.databaseId} lands it again.`);
    return 0;
  }
  console.log('No landing was dropped while it waited.');
  return 0;
}

/**
 * Lands from this PC a queued commit that CI checked but could not push itself, because it changes a workflow file
 * (GitHub refuses that to the workflow's token): only when its `gate` passed and the pull request is still at the head that
 * was squashed. A base that moved meanwhile takes it as checked when it moved by words or by files the commit does not
 * touch (`onMoved`, the land job's rule): trains land every half hour, and a workflow change checked alone would
 * otherwise never find the base still. The same atomic push as the land job of ci.yml.
 */
async function finish(number) {
  // The pre-push hook's owed-work guard (scripts/owed.mjs) counts this very pull request once its branch moves to the
  // landed commit, and refused the push onto phase-1-base (#559). Finishing a checked landing is what the guard waits for.
  process.env.FABER_IGNORE_OWED = '1';
  const queue = `queue/pr-${number}`;
  git('fetch', '--quiet', 'origin', `+refs/heads/${BASE}:refs/remotes/origin/${BASE}`, `+refs/heads/${queue}:refs/remotes/origin/${queue}`, '+refs/heads/main:refs/remotes/origin/main');
  const sha = git('rev-parse', `origin/${queue}`).trim();
  const gate = run('gh', ['api', `repos/{owner}/{repo}/commits/${sha}/check-runs?check_name=gate`, '--jq', '[.check_runs[] | select(.conclusion == "success")] | length']).trim();
  if (gate === '0') {
    console.error(`The gate of ${sha.slice(0, 9)} has not passed (yet): nothing lands.`);
    return 1;
  }
  const base = git('rev-parse', `origin/${BASE}`).trim();
  let landing = sha;
  let checkBase = false;
  if (!ancestor(base, sha)) {
    const batch = stacked(git('log', '--reverse', LOG, `${base}..${sha}`));
    const moved = batch?.length === 1 ? await onMoved(base, sha, batch, { ownWorkflow: true }) : null;
    if (!moved) {
      console.error(`${BASE} moved after #${number} was queued, in files it touches: queue it again with node scripts/land.mjs ${number}.`);
      return 1;
    }
    landing = moved.landing;
    checkBase = moved.kind === 'disjoint';
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
  git('push', '--quiet', `--force-with-lease=${branch}:${oid}`, 'origin', `${landing}:${branch}`);
  execFileSync(process.execPath, ['-e', 'setTimeout(() => {}, 5000)'], { windowsHide: true });
  try {
    git('push', '--quiet', 'origin', `${landing}:refs/heads/${BASE}`);
  } catch (error) {
    git('push', '--quiet', `--force-with-lease=${branch}:${landing}`, 'origin', `${oid}:${branch}`);
    console.error(String(error.stderr || error.message).trim());
    console.error(`${BASE} refused it, so #${number}'s branch is back where it was: queue it again.`);
    return 1;
  }
  if (checkBase || !ancestor(base, 'origin/main')) {
    // Independent changes put together were never checked whole: main follows once phase-1-base's own check is green.
    run('gh', ['workflow', 'run', 'ci.yml', '--ref', BASE]);
    console.log('main follows once the phase-1-base check is green.');
  } else if (ancestor('origin/main', landing)) {
    git('push', '--quiet', 'origin', `${landing}:refs/heads/main`);
  }
  git('push', '--quiet', 'origin', `:refs/heads/${queue}`);
  console.log(`#${number} landed as ${landing.slice(0, 9)} on ${BASE}.`);
  return 0;
}

// Real paths on both sides: a temporary folder may be a link (macOS's /var is /private/var), and Node gives the module its real path.
if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(import.meta.filename)) {
  const args = process.argv.slice(2);
  const number = args.find((a) => /^\d+$/.test(a));
  const value = (flag) => (args.includes(flag) ? args[args.indexOf(flag) + 1] : undefined);
  const self = value('--self');
  if (args.includes('--sweep')) {
    process.exitCode = sweep();
  } else if (args.includes('--train')) {
    process.exitCode = await train({ after: value('--after'), self, only: value('--only')?.split(',') ?? null });
  } else if (!number) {
    console.error('usage: node scripts/land.mjs <pull request> [--dry-run] [--no-verify] [--alone] [--head <commit>] | <pull request> --finish | --sweep');
    process.exitCode = 2;
  } else if (args.includes('--finish')) {
    process.exitCode = await finish(number);
  } else if (args.includes('--land')) {
    process.exitCode = await landQueued(number, value('--land'), self);
  } else if (args.includes('--split')) {
    // Exit 3: queued again alone, or its train's first half rides again, so the report job blames nobody yet.
    process.exitCode = (await split(number, value('--split'), self)) ? 3 : 0;
  } else {
    const at = args.indexOf('--head');
    process.exitCode = await land(number, args.includes('--dry-run'), at >= 0 ? args[at + 1] : undefined, args);
  }
}
