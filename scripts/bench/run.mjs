// The fair benchmark of R113 (docs/superpowers/specs/2026-10-14-test-system-rework-design.md, section 3): runs each
// measurement of Faber and of TL-ADE through the meter (`faber-bench`, one job object per run, an idle PC) once to warm up
// and then `--runs` times, appends every run to docs/research/bench/<date>.jsonl and prints the medians as a table.
// Compilation is never measured (R115): the warm-up run builds what the measured ones reuse.
//
//   node scripts/bench/run.mjs [--only suites|matched] [--runs 3] [--tlade <folder>] [--bench <faber-bench.exe>]
//
// Children start with no shell: the meter is given the real programs (cargo, node).
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/** The 8 matched scenarios of the spec's section 3: the same behaviour proved on both sides. */
export const MATCHED = [
  { id: 1, behaviour: 'A turn with a recorded CLI streams and ends', tlade: ['tests/adapter-claude.test.ts', 'tests/fake-cli.test.ts'], faber: ['claude_session::', 'codex_session::'] },
  { id: 2, behaviour: 'Resume continues the same session', tlade: ['tests/adapters/codex_resume.test.ts', 'tests/adapter-claude.test.ts'], faber: ['session_resume::'] },
  { id: 3, behaviour: 'Quota is read from each CLI', tlade: ['tests/official_quota.test.ts'], faber: ['quota::', 'budget::'] },
  { id: 4, behaviour: 'Hitting the quota pauses and resumes after the reset', tlade: ['tests/quota_pause_resume.test.ts', 'tests/engine-quota.test.ts'], faber: ['waiting::'] },
  { id: 5, behaviour: 'The router picks a model by quota and cost', tlade: ['tests/model_routing.test.ts', 'tests/model_chains.test.ts'], faber: ['route::', 'pacing::', 'auto::'] },
  { id: 6, behaviour: 'Unknown or blocked billing is never picked', tlade: ['tests/model_routing.test.ts', 'tests/adapters/claude.test.ts'], faber: ['budget::', 'paid_guard::'] },
  { id: 7, behaviour: 'Tokens and cost are recorded, never invented', tlade: ['tests/adapters/claude.test.ts', 'tests/usage_by_company.test.ts'], faber: ['turn_usage::'] },
  { id: 8, behaviour: 'An npm .cmd shim resolves to the real program', tlade: ['tests/doctor.test.ts', 'tests/parity/windows-durability.test.ts'], faber: ['engine::cli_lookup_follows_npm_cmd_shims'] },
];

/** What one plan measures: a label, the folder it runs in, the program and its arguments. */
export function plan(only, tlade) {
  const vitest = ['node_modules/vitest/vitest.mjs', 'run'];
  const suites = [
    { label: 'tlade suite', dir: tlade, program: 'node', args: vitest },
    { label: 'faber suite', dir: root, program: 'cargo', args: ['t', '--all'] },
  ];
  // A Faber scenario with several test modules is one run per module (`--filter` is a substring of the test's path in the
  // one engine test program), summed: its runner's start counts once per module, which can only favour TL-ADE.
  const matched = MATCHED.flatMap((m) => [
    { label: `tlade matched ${m.id}`, scenario: m.id, dir: tlade, program: 'node', args: [...vitest, ...new Set(m.tlade)] },
    ...m.faber.map((f) => ({ label: `faber matched ${m.id} ${f}`, scenario: m.id, dir: root, program: 'cargo', args: ['t', '--suite', 'rust', '--filter', f] })),
  ]);
  return only === 'suites' ? suites : only === 'matched' ? matched : [...suites, ...matched];
}

/** The median of a list of numbers. */
export function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

const FIELDS = ['wall_s', 'procs_started', 'cpu_s', 'peak_commit_mb', 'written_mb'];

/** The medians of each label's measured runs, in the order the labels first appear. */
export function medians(lines) {
  const by = new Map();
  for (const line of lines.filter((l) => !l.warmup)) {
    if (!by.has(line.label)) by.set(line.label, []);
    by.get(line.label).push(line);
  }
  return [...by].map(([label, runs]) => ({
    label,
    runs: runs.length,
    exit: runs.map((r) => r.exit),
    leaked: runs.reduce((n, r) => n + (r.leaked ?? 0), 0),
    ...Object.fromEntries(FIELDS.map((f) => [f, median(runs.map((r) => r[f]))])),
  }));
}

/** The medians as a Markdown table. */
export function table(rows) {
  const head = '| Run | Wall s | Processes | CPU s | Peak commit MB | Written MB | Exits | Left running |\n|---|---|---|---|---|---|---|---|';
  const fmt = (r) =>
    `| ${r.label} | ${r.wall_s} | ${r.procs_started} | ${r.cpu_s} | ${r.peak_commit_mb} | ${r.written_mb} | ${r.exit.join(', ')} | ${r.leaked} |`;
  return [head, ...rows.map(fmt)].join('\n');
}

function main() {
  const args = process.argv.slice(2);
  const flag = (name, fallback) => {
    const at = args.indexOf(name);
    return at >= 0 ? args[at + 1] : fallback;
  };
  const only = flag('--only', 'all');
  const runs = Number(flag('--runs', '3'));
  const tlade = resolve(flag('--tlade', join(root, '..', 'TL-ADE')));
  const target = process.env.CARGO_TARGET_DIR || join(root, 'target');
  const bench = flag('--bench', join(target, 'debug', 'faber-bench.exe'));
  if (!existsSync(bench)) throw new Error(`no meter at ${bench}: cargo build -p faber-testkit --bin faber-bench`);
  if (!existsSync(join(tlade, 'node_modules', 'vitest'))) throw new Error(`no TL-ADE with its dependencies at ${tlade}`);
  const folder = join(root, 'docs', 'research', 'bench');
  mkdirSync(folder, { recursive: true });
  const out = join(folder, `${new Date().toISOString().slice(0, 10)}.jsonl`);
  const lines = [];
  for (const step of plan(only, tlade)) {
    for (let n = 0; n <= runs; n++) {
      const warmup = n === 0;
      const label = warmup ? `${step.label} (warm-up)` : step.label;
      const meter = ['--label', label, '--dir', step.dir, '--out', warmup ? `${out}.warmup` : out, '--', step.program, ...step.args];
      const run = spawnSync(bench, meter, { encoding: 'utf8', windowsHide: true, stdio: ['ignore', 'pipe', 'inherit'] });
      if (run.status !== 0) throw new Error(`the meter failed on ${label}`);
      const line = JSON.parse(run.stdout.trim().split('\n').pop());
      lines.push({ ...line, warmup });
      console.error(`${label}: ${line.wall_s} s, exit ${line.exit}`);
    }
  }
  console.log(table(medians(lines)));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
