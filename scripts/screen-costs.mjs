// How many seconds each screen scenario takes on CI, so `cargo t --shard` deals the parts evenly (R-next). The seconds
// come from the run.log of every screen part of the newest green runs, so the table never ages: CI's `costs` job hands
// them to its parts, and the copy in the repository (testkit/screen-costs.json) is what a local run and a CI run without
// those logs deal by.
//
//   node scripts/screen-costs.mjs fetch            prints the table of the newest green runs, or the repository's copy
//   node scripts/screen-costs.mjs fetch --write    writes it to testkit/screen-costs.json
//   node scripts/screen-costs.mjs parse <log>...   prints the table of these run.log files
//
// `fetch` never fails: without `gh`, a network or green runs it prints the repository's copy and says why on stderr.
// Nothing goes through a shell: `gh` runs with an argument list.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const TABLE = fileURLToPath(new URL('../testkit/screen-costs.json', import.meta.url));
/** The newest green runs whose seconds count; each scenario takes the middle value of its runs. */
const RUNS = 3;
/** How many runs back `fetch` looks for green ones with every screen part. */
const LOOK_BACK = 40;
const PARTS = 8;

/** Seconds per scenario in one run.log: the `pass  screen  <name> (<seconds> s)` lines. */
export function parseLog(text) {
  const seconds = new Map();
  for (const line of text.split(/\r?\n/)) {
    const m = /^pass {2}screen {2}(.+?) \((\d+(?:\.\d+)?) s\)(?: {2}|$)/.exec(line);
    if (m) seconds.set(m[1], Number(m[2]));
  }
  return seconds;
}

/** One table from several runs: each scenario takes the middle of its values (the upper one of two), rounded up. */
export function merge(runs) {
  const all = new Map();
  for (const run of runs) for (const [name, s] of run) all.set(name, [...(all.get(name) ?? []), s]);
  const seconds = {};
  for (const name of [...all.keys()].sort()) {
    const values = all.get(name).sort((a, b) => a - b);
    seconds[name] = Math.max(1, Math.ceil(values[Math.floor(values.length / 2)]));
  }
  return seconds;
}

export function table(runs, seconds) {
  return {
    about:
      'Seconds each screen scenario took on CI (the middle of the newest green runs), which `cargo t --shard` deals the parts by (R-next). Written by `node scripts/screen-costs.mjs fetch --write`; CI reads the newest runs itself. A scenario missing here counts as 5 s.',
    runs,
    seconds,
  };
}

const gh = (...args) => execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 64 << 20 });

/** The table of the newest green runs that kept a log for every screen part. */
function fetchTable() {
  const repo = process.env.GITHUB_REPOSITORY ?? JSON.parse(gh('repo', 'view', '--json', 'nameWithOwner')).nameWithOwner;
  // Pull requests run a merge with code that may never land, so only the shared branches and the queue count.
  const runs = JSON.parse(
    gh('run', 'list', '-R', repo, '--workflow', 'ci.yml', '--status', 'success', '--limit', String(LOOK_BACK), '--json', 'databaseId,event'),
  ).filter((run) => run.event !== 'pull_request');
  const used = [];
  const logs = [];
  const dir = mkdtempSync(join(tmpdir(), 'screen-costs-'));
  try {
    for (const { databaseId: id } of runs) {
      if (used.length === RUNS) break;
      const names = gh('api', `repos/${repo}/actions/runs/${id}/artifacts?per_page=100`, '--jq', '.artifacts[] | select(.expired | not) | .name')
        .split('\n')
        .filter((name) => name.startsWith('log-screen-'));
      if (names.length < PARTS) continue;
      const into = join(dir, String(id));
      gh('run', 'download', String(id), '-R', repo, '-D', into, ...names.flatMap((name) => ['-n', name]));
      const run = new Map();
      for (const name of names) {
        const log = join(into, name, 'run.log');
        if (existsSync(log)) for (const [scenario, s] of parseLog(readFileSync(log, 'utf8'))) run.set(scenario, s);
      }
      if (run.size === 0) continue;
      used.push(id);
      logs.push(run);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  if (used.length === 0) throw new Error(`no green run of the last ${LOOK_BACK} kept the logs of its ${PARTS} screen parts`);
  return table(used, merge(logs));
}

if (process.argv[1]?.endsWith('screen-costs.mjs')) {
  const [command, ...rest] = process.argv.slice(2);
  if (command === 'parse') {
    process.stdout.write(`${JSON.stringify(table([], merge(rest.map((file) => parseLog(readFileSync(file, 'utf8'))))), null, 2)}\n`);
  } else if (command === 'fetch') {
    let text;
    try {
      text = `${JSON.stringify(fetchTable(), null, 2)}\n`;
    } catch (e) {
      process.stderr.write(`Dealing by the table in the repository: ${e.message.split('\n')[0]}\n`);
      text = readFileSync(TABLE, 'utf8');
    }
    if (rest.includes('--write')) writeFileSync(TABLE, text);
    else process.stdout.write(text);
  } else {
    process.stderr.write('use: node scripts/screen-costs.mjs fetch [--write] | parse <run.log>...\n');
    process.exit(2);
  }
}
