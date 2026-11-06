import { execFileSync } from 'node:child_process';
import { appendFileSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

// Checks that fail by chance leave the gate by themselves (lean plan D5). On CI the runner runs a failed check once more;
// one that then passes is flaky and its name lands in `flaky-report.json` (testkit/src/quarantine.rs). The gate job puts
// the reports of its run together here and keeps the list in the file `flaky.json` of the branch `ci-state`; the next
// runs read it (`fetch`) and do not fail on a listed check, except the nightly run. A listed check returns to the gate
// after CLEAN_NIGHTS nightly runs in a row in which it passed the first time. Nobody edits the list.
//
//   node scripts/flaky.mjs fetch <file>   writes the list for the runner; sets FABER_FLAKY_FILE for the next steps
//   node scripts/flaky.mjs update <dir>   gate job: reads every flaky-report.json under <dir>, saves the list, tells the night
export const CLEAN_NIGHTS = 5;
/** A listed check no night has seen for this many days is gone (renamed or removed), so it leaves the list. */
export const GONE_AFTER_DAYS = 14;
const BRANCH = 'ci-state';
const FILE = 'flaky.json';

export const empty = () => ({ checks: {} });
export const parse = (text) => {
  try {
    const checks = JSON.parse(text)?.checks;
    return { checks: checks && typeof checks === 'object' && !Array.isArray(checks) ? checks : {} };
  } catch { return empty(); }
};
const merge = (reports) => {
  const all = { flaky: new Set(), clean: new Set(), failed: new Set() };
  for (const report of reports) for (const key of Object.keys(all)) for (const name of report?.[key] ?? []) all[key].add(name);
  return all;
};
const days = (from, to) => (Date.parse(to) - Date.parse(from)) / 864e5;

/**
 * The list after a run. Pure: `reports` are the parsed flaky-report.json files of the run, `today` an ISO date.
 * - A check that failed and passed on the second try joins the list (or starts counting again).
 * - On the nightly run only: a listed check that passed the first time gains a clean night, one that failed or was flaky
 *   again, or that no report mentions, loses its nights; one with CLEAN_NIGHTS clean nights in a row returns to the gate.
 *   A listed check no night has seen for GONE_AFTER_DAYS days is dropped. A night counts once whatever the reruns.
 */
export function step(state, reports, { nightly, today }) {
  const seen = merge(reports);
  const checks = Object.fromEntries(Object.entries(state.checks).map(([name, entry]) => [name, { ...entry }]));
  const added = [];
  const returned = [];
  const gone = [];
  for (const name of seen.flaky) {
    if (!checks[name]) added.push(name);
    checks[name] = { since: checks[name]?.since ?? today, clean: 0, seen: nightly ? today : checks[name]?.seen };
  }
  if (nightly) {
    for (const name of Object.keys(checks)) {
      if (seen.flaky.has(name)) continue;
      if (seen.failed.has(name)) { checks[name].clean = 0; checks[name].seen = today; continue; }
      if (seen.clean.has(name)) {
        // One clean night counts once, even when the daily run is run again.
        if (checks[name].seen !== today) checks[name].clean = (checks[name].clean ?? 0) + 1;
        checks[name].seen = today;
        if (checks[name].clean >= CLEAN_NIGHTS) { delete checks[name]; returned.push(name); }
      } else {
        // A night that says nothing about the check (a missing report, a suite that did not start) proves nothing.
        checks[name].clean = 0;
        if (days(checks[name].seen ?? checks[name].since ?? today, today) > GONE_AFTER_DAYS) { delete checks[name]; gone.push(name); }
      }
    }
  } else {
    for (const name of seen.failed) if (checks[name] && !seen.flaky.has(name)) checks[name].clean = 0;
  }
  return { state: { checks }, added, returned, gone };
}

/** The morning words: which checks are off the gate, and how close each one is to returning. */
export function summarize({ state, added = [], returned = [], gone = [] }) {
  const names = Object.keys(state.checks).sort();
  const lines = [names.length ? `Checks that fail by chance (${names.length}). They run, but only the nightly run fails on them:` : 'No check fails by chance.'];
  for (const name of names) {
    const { since, clean = 0 } = state.checks[name];
    lines.push(`- ${name} (since ${since}; ${clean} of ${CLEAN_NIGHTS} clean nights)`);
  }
  if (added.length) lines.push('', `Joined the list: ${added.join('; ')}`);
  if (returned.length) lines.push('', `Back on the gate after ${CLEAN_NIGHTS} clean nights: ${returned.join('; ')}`);
  if (gone.length) lines.push('', `Left the list, no longer seen: ${gone.join('; ')}`);
  return lines.join('\n');
}

const run = (cmd, args, options = {}) => execFileSync(cmd, args, { encoding: 'utf8', windowsHide: true, maxBuffer: 32 << 20, ...options }).trim();
const git = (...args) => run('git', args);
const present = () => run('git', ['ls-remote', '--heads', 'origin', BRANCH]) !== '';
/** The saved list and the commit it came from (null before the first one). A fetch that fails throws: it must not look empty. */
function load() {
  if (!present()) return { parent: null, state: empty() };
  git('fetch', '--depth=1', 'origin', BRANCH);
  const parent = git('rev-parse', 'FETCH_HEAD');
  return { parent, state: parse(git('show', `${parent}:${FILE}`)) };
}
function save(state, parent, message) {
  const blob = run('git', ['hash-object', '-w', '--stdin'], { input: `${JSON.stringify(state, null, 2)}\n` });
  const tree = run('git', ['mktree'], { input: `100644 blob ${blob}\t${FILE}\n` });
  const identity = ['-c', 'user.name=github-actions[bot]', '-c', 'user.email=41898282+github-actions[bot]@users.noreply.github.com'];
  const commit = run('git', [...identity, 'commit-tree', tree, ...(parent ? ['-p', parent] : []), '-m', message]);
  git('push', 'origin', `${commit}:refs/heads/${BRANCH}`);
}
function reportsUnder(dir) {
  const found = [];
  const walk = (at) => {
    for (const name of readdirSync(at)) {
      const path = join(at, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (name === 'flaky-report.json') found.push(JSON.parse(readFileSync(path, 'utf8')));
    }
  };
  try { walk(dir); } catch { /* no logs: nothing to learn */ }
  return found;
}
function say(text) {
  console.log(text);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${text}\n`);
}
/** One open issue, labelled `flaky`, always says which checks are off the gate; it closes when none is. */
function tell(text, any) {
  const title = 'Checks that fail by chance';
  try {
    run('gh', ['label', 'create', 'flaky', '--color', 'FBCA04', '--description', 'Checks that failed and then passed on the same commit'], { stdio: 'ignore' });
  } catch { /* the label exists */ }
  const open = run('gh', ['issue', 'list', '--label', 'flaky', '--state', 'open', '--limit', '1', '--json', 'number', '--jq', '.[0].number // empty']);
  if (any && open) run('gh', ['issue', 'edit', open, '--body', text]);
  else if (any) run('gh', ['issue', 'create', '--label', 'flaky', '--title', title, '--body', text]);
  else if (open) run('gh', ['issue', 'close', open, '--comment', 'Every check passes by itself again.']);
}

export function main(argv = process.argv.slice(2)) {
  const [command, target] = argv;
  try {
    if (command === 'fetch' && target) {
      let state = empty();
      // The list helps a run, never blocks it: without it every failure counts, as before.
      try { state = load().state; } catch (e) { console.warn(`flaky list not read: ${String(e.stderr || e.message)}`); }
      mkdirSync(dirname(resolve(target)), { recursive: true });
      writeFileSync(target, JSON.stringify(state));
      if (process.env.GITHUB_ENV) appendFileSync(process.env.GITHUB_ENV, `FABER_FLAKY_FILE=${resolve(target)}\n`);
      return 0;
    }
    if (command === 'update' && target) {
      const reports = reportsUnder(target);
      const nightly = process.env.GITHUB_EVENT_NAME === 'schedule';
      const today = new Date().toISOString().slice(0, 10);
      for (let attempt = 1; ; attempt++) {
        const { parent, state } = load();
        const result = step(state, reports, { nightly, today });
        const changed = JSON.stringify(result.state) !== JSON.stringify(state);
        const text = summarize(result);
        try {
          if (changed) save(result.state, parent, `flaky: ${text.split('\n')[0]}`);
        } catch (e) {
          // Another run saved first: read the list again and apply this run to it.
          if (attempt < 4 && /non-fast-forward|fetch first|rejected/i.test(String(e.stderr || e.message))) continue;
          throw e;
        }
        say(text);
        if (changed || nightly) tell(text, Object.keys(result.state.checks).length > 0);
        return 0;
      }
    }
    console.error('usage: node scripts/flaky.mjs fetch <file> | update <logs-dir>');
    return 2;
  } catch (e) {
    // Bookkeeping never turns a run red: the gate has its own verdict.
    console.warn(`flaky: ${String(e.stderr || e.message)}`);
    return 0;
  }
}
if (process.argv[1] && resolve(process.argv[1]) === resolve(import.meta.filename)) process.exitCode = main();
