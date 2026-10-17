// Lands a pull request through the queue (R162): squashed into one commit on top of the newest phase-1-base and numbered
// there, so its numbers are given only when it lands and two pull requests never wait on each other's
// (docs/design.md#numbers).
//
//   node scripts/land.mjs <pull request> [--dry-run] [--head <commit>]
//
// It squashes the pull request onto origin/phase-1-base without touching the working tree (`git merge-tree`), names the
// commit after the pull request's title and number, numbers it in a throwaway worktree (`node scripts/numbers.mjs assign
// --amend`: docs/next/*.json fold into the locales and the decisions, and every placeholder takes the next free number),
// and pushes it to queue/pr-<number>. CI checks that very commit; the land job of ci.yml puts it on phase-1-base, moves the
// pull request's branch to it so GitHub shows it merged, and lets main follow. When the base moves while the checks run,
// the land job runs this again with --head, the head it squashed before, so the numbers are given again on the new base
// instead of a rebase that would collide. A conflict with today's base stops here, with its files. --dry-run shows the
// commit it would queue and pushes nothing.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
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
 * the usual case and never a contradiction (docs/design.md#numbers). */
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

const run = (cmd, args, options = {}) => execFileSync(cmd, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true, ...options });
const git = (...args) => run('git', args);

function land(number, dry, given) {
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
  const squashed = run('git', ['commit-tree', tree, '-p', base, '-F', '-'], { input: message(pr.title, pr.number, pr.body, head), stdio: ['pipe', 'pipe', 'pipe'] }).trim();
  const commit = numberedCommit(squashed);
  if (!commit) return 1;
  if (dry) {
    console.log(git('show', '--stat', '--format=%B', commit).trim());
    console.log('Dry run: nothing was pushed.');
    return 0;
  }
  try {
    git('push', '--quiet', `--force-with-lease=${queue}:${queued}`, 'origin', `${commit}:${queue}`);
  } catch (error) {
    console.error(String(error.stderr || error.message).trim());
    console.error(`queue/pr-${pr.number} was not replaced: when another landing of #${pr.number} queued it meanwhile, that newer one stands.`);
    return 1;
  }
  console.log(`#${pr.number} is queued as ${commit.slice(0, 9)} on queue/pr-${pr.number}. CI checks it, lands it and marks the pull request merged.`);
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
  git('push', '--quiet', '--atomic', `--force-with-lease=refs/heads/${pr.headRefName}:${oid}`, 'origin', `${sha}:refs/heads/${pr.headRefName}`, `${sha}:refs/heads/${BASE}`);
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

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'land.mjs'))) {
  const args = process.argv.slice(2);
  const number = args.find((a) => /^\d+$/.test(a));
  if (!number) {
    console.error('usage: node scripts/land.mjs <pull request> [--dry-run] [--head <commit>] | <pull request> --finish');
    process.exitCode = 2;
  } else if (args.includes('--finish')) {
    process.exitCode = finish(number);
  } else {
    const at = args.indexOf('--head');
    process.exitCode = land(number, args.includes('--dry-run'), at >= 0 ? args[at + 1] : undefined);
  }
}
