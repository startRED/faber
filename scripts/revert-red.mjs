import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { resolve } from 'node:path';

// `parentGate` is the parent's gate job, not its whole run: advisory jobs (Electron) may fail beside a green gate.
// `fixedLater` means a commit landed after this one already has a green gate, so a revert would undo work it relies on.
export function decide({ subject, parentGate, reverted, fixedLater, runnerUnavailable }) {
  if (/^revert/i.test(subject) || runnerUnavailable) return 'skip';
  if (reverted || fixedLater) return 'stale';
  return parentGate === 'success' ? 'revert' : 'wait';
}
export const revertsOf = (log, sha) => log.split('\0')
  .some((m) => m.split('\n').some((line) => line.trim() === `Reverts: ${sha}`) || m.includes(`This reverts commit ${sha}.`));
export const pullRequest = (message) => message.match(/^Pull-request:\s*#(\d+)\s*$/im)?.[1];
export const revertMessage = (subject, sha, url) => `revert: ${subject}\n\nFailed run: ${url}\n\nReverts: ${sha}\n`;
export const notice = (sha, url) => `The checks failed: ${url}\n\n${sha} was reverted. Ship the fix again with \`node scripts/ship.mjs\`.`;

const run = (cmd, args, options = {}) => execFileSync(cmd, args, { encoding: 'utf8', windowsHide: true, maxBuffer: 32 << 20, ...options }).trim();
const git = (...args) => run('git', args);
const gh = (...args) => run('gh', args);
/** The gate job's conclusion in the newest completed push or dispatched ci.yml run of a commit, or null. */
function gateOf(repo, sha) {
  const runs = JSON.parse(gh('api', `repos/${repo}/actions/workflows/ci.yml/runs?branch=phase-1-base&head_sha=${sha}&status=completed&per_page=20`))
    .workflow_runs.filter((r) => r.head_sha === sha && ['push', 'workflow_dispatch'].includes(r.event));
  for (const r of runs) {
    const gate = JSON.parse(gh('api', `repos/${repo}/actions/runs/${r.id}/jobs?per_page=100`)).jobs.find((j) => j.name === 'gate');
    if (gate?.conclusion) return gate.conclusion;
  }
  return null;
}
function summary(text) {
  console.log(text);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${text}\n`);
}
export function main() {
  try {
    const { GITHUB_SHA: sha, GITHUB_REPOSITORY: repo, GITHUB_SERVER_URL: server = 'https://github.com', GITHUB_RUN_ID: runId } = process.env;
    if (!sha || !repo || !runId) throw Error('GITHUB_SHA, GITHUB_REPOSITORY and GITHUB_RUN_ID are required.');
    const url = `${server}/${repo}/actions/runs/${runId}`;
    git('fetch', 'origin', 'phase-1-base');
    const subject = git('show', '-s', '--format=%s', sha);
    const message = git('show', '-s', '--format=%B', sha);
    const parent = git('rev-parse', `${sha}^`);
    const runnerUnavailable = process.env.RUNNER_UNAVAILABLE === 'true';
    const reverted = revertsOf(git('log', '--format=%B%x00', `${sha}..origin/phase-1-base`), sha);
    const early = decide({ subject, reverted, runnerUnavailable });
    if (early === 'skip' || early === 'stale') { summary(`${early}: ${sha}`); return 0; }
    const later = git('rev-list', `${sha}..origin/phase-1-base`).split('\n').filter(Boolean);
    const fixedLater = later.some((c) => gateOf(repo, c) === 'success');
    const decision = decide({ subject, parentGate: gateOf(repo, parent), reverted, fixedLater, runnerUnavailable });
    if (decision === 'skip' || decision === 'stale') { summary(`${decision}: ${sha}`); return 0; }
    if (decision === 'wait') {
      gh('api', '--method', 'POST', `repos/${repo}/commits/${sha}/comments`, '-f', `body=The base is red and the culprit is not yet known. Failed run: ${url}`);
      summary(`wait: ${sha}; parent result does not identify the culprit`); return 0;
    }
    git('checkout', '--detach', 'origin/phase-1-base');
    try { git('revert', '--no-edit', sha); }
    catch (e) { summary(`revert failed: ${sha}; resolve the conflict manually. ${String(e.stderr || e.message)}`); return 1; }
    run('git', ['commit', '--amend', '-F', '-'], { input: revertMessage(subject, sha, url) });
    try { git('push', 'origin', 'HEAD:refs/heads/phase-1-base'); }
    catch (e) {
      if (!/non-fast-forward|fetch first/i.test(String(e.stderr || e.message))) throw e;
      git('fetch', 'origin', 'phase-1-base');
      const gained = git('rev-list', 'HEAD^..origin/phase-1-base').split('\n').filter(Boolean);
      if (gained.some((c) => gateOf(repo, c) === 'success')) {
        summary(`stale: a commit landed after ${sha} is already green`); return 0;
      }
      if (revertsOf(git('log', '--format=%B%x00', 'HEAD^..origin/phase-1-base'), sha)) {
        summary(`stale: ${sha} was already reverted while the base moved`); return 0;
      }
      try { git('rebase', 'origin/phase-1-base'); }
      catch (conflict) { summary(`revert failed: ${sha}; base moved and the revert conflicts. ${String(conflict.stderr || conflict.message)}`); return 1; }
      git('push', 'origin', 'HEAD:refs/heads/phase-1-base');
    }
    const landed = git('rev-parse', 'HEAD');
    gh('workflow', 'run', 'ci.yml', '--ref', 'phase-1-base');
    const text = notice(sha, url);
    gh('api', '--method', 'POST', `repos/${repo}/commits/${sha}/comments`, '-f', `body=${text}`);
    const pr = pullRequest(message);
    if (pr) gh('pr', 'comment', pr, '--body', text);
    summary(`revert: ${sha} reverted as ${landed}`);
    return 0;
  } catch (e) { summary(`revert error: ${String(e.stderr || e.message)}`); return 1; }
}
if (process.argv[1] && resolve(process.argv[1]) === resolve(import.meta.filename)) process.exitCode = main();
