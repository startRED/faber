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
import { join, resolve } from 'node:path';

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
  const pr = JSON.parse(run('gh', ['pr', 'view', number, '--json', 'number,title,body,state,isDraft,baseRefName,isCrossRepository,headRefOid']));
  const refused = refusal(pr);
  if (refused) {
    console.error(`#${pr.number} cannot land: ${refused}.`);
    return 1;
  }
  const head = given ?? pr.headRefOid;
  git('fetch', '--quiet', 'origin', BASE, given ?? `refs/pull/${pr.number}/head`);
  const base = git('rev-parse', `origin/${BASE}`).trim();
  let tree;
  try {
    tree = git('merge-tree', '--write-tree', '--name-only', '--no-messages', base, head).split('\n')[0].trim();
  } catch (error) {
    if (error.status !== 1) throw error;
    console.error(`#${pr.number} conflicts with ${BASE} in:\n  ${conflicts(String(error.stdout)).join('\n  ')}`);
    console.error(`Merge origin/${BASE} into its branch, settle them, push, and run this again.`);
    return 1;
  }
  const squashed = run('git', ['commit-tree', tree, '-p', base, '-F', '-'], { input: message(pr.title, pr.number, pr.body, head), stdio: ['pipe', 'pipe', 'pipe'] }).trim();
  const commit = numberedCommit(squashed);
  if (!commit) return 1;
  if (dry) {
    console.log(git('show', '--stat', '--format=%B', commit).trim());
    console.log('Dry run: nothing was pushed.');
    return 0;
  }
  git('push', '--quiet', '--force', 'origin', `${commit}:refs/heads/queue/pr-${pr.number}`);
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

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'land.mjs'))) {
  const args = process.argv.slice(2);
  const number = args.find((a) => /^\d+$/.test(a));
  if (!number) {
    console.error('usage: node scripts/land.mjs <pull request> [--dry-run] [--head <commit>]');
    process.exitCode = 2;
  } else {
    const at = args.indexOf('--head');
    process.exitCode = land(number, args.includes('--dry-run'), at >= 0 ? args[at + 1] : undefined);
  }
}
