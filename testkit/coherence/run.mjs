// Coherence traps (R145): change requests that tempt an agent into a second way of doing what Faber already does
// (traps.json). Each trap goes to a fresh CLI agent twice, in a copy of HEAD with the product map and in one without it;
// the agent answers with a JSON plan and edits nothing. A second agent, which sees neither the repository nor the
// condition, scores the plan against the trap's rubric. It spends real quota, so it never runs inside `cargo t`.
//
//   node testkit/coherence/run.mjs [--cli codex|claude] [--model …] [--effort medium] [--reps 1] [--jobs 3]
//                                  [--only id,id] [--out target/coherence/<date>]
//   node testkit/coherence/run.mjs --dry      builds both copies and checks the map-free one, with no agent and no cost
//   node testkit/coherence/run.mjs --rejudge --out <dir>    scores the plans in <dir> again with the current rubrics
//
// The copies live in the system temp folder, outside any git repository, so `git` cannot reach the map's history, and
// are deleted at the end. Every child starts hidden (windowsHide), so none opens a window.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const flags = Object.fromEntries(process.argv.slice(2).join(' ').split('--').filter(Boolean).map((f) => f.trim().split(/\s+/)));
const cli = flags.cli ?? 'codex';
const model = flags.model ?? (cli === 'codex' ? 'gpt-6.1-sol' : 'sonnet');
const effort = flags.effort ?? 'medium';
const reps = Number(flags.reps ?? 1);
const jobs = Number(flags.jobs ?? 3);
const out = path.resolve(repo, flags.out ?? `target/coherence/${new Date().toISOString().slice(0, 10)}`);
const { traps, without } = JSON.parse(fs.readFileSync(path.join(here, 'traps.json'), 'utf8'));
const chosen = flags.only ? traps.filter((t) => flags.only.split(',').includes(t.id)) : traps;

// API-equivalent US$ per million tokens (uncached input, cached input, output), from `engine/src/usage.rs`.
const PRICES = { 'gpt-6.1-sol': [2.0, 0.1, 10.0], 'gpt-6-luna': [0.1, 0.01, 0.5] };

const PLAN = {
  type: 'object',
  additionalProperties: false,
  required: ['summary', 'files', 'new_settings', 'new_controls', 'new_texts', 'reuses', 'raise_with_owner'],
  properties: {
    summary: { type: 'string' },
    files: { type: 'array', items: obj({ path: 'string', change: 'string' }) },
    new_settings: { type: 'array', items: { type: 'string' } },
    new_controls: { type: 'array', items: obj({ screen: 'string', control: 'string' }) },
    new_texts: { type: 'array', items: obj({ key: 'string', english: 'string' }) },
    reuses: { type: 'array', items: { type: 'string' } },
    raise_with_owner: { type: 'string' },
  },
};
const VERDICT = { ...obj({ verdict: 'string', reason: 'string' }) };
VERDICT.properties.verdict = { type: 'string', enum: ['PASS', 'FAIL'] };

function obj(fields) {
  const properties = Object.fromEntries(Object.entries(fields).map(([k, type]) => [k, { type }]));
  return { type: 'object', additionalProperties: false, required: Object.keys(fields), properties };
}

const ask = (request) => `The owner of this repository asks for the change below. Plan it the way you would before writing code: read what you need in the repository and follow its instructions for agents. Do not edit files and do not run builds or tests. Your final answer is only the JSON plan the output schema describes: the concrete changes you would make (files, settings, controls, screen texts, what you reuse), and anything you would raise with the owner instead of building it as asked.

Request: ${request}`;

const judge = (trap, plan) => `You grade one implementation plan against a rubric. Use only the rubric and the plan; do not guess what the code base holds beyond them. The verdict is PASS only when the plan meets the PASS rule and no FAIL condition; a plan that meets both fails.

Request: ${trap.request}
PASS when: ${trap.pass}
FAIL when: ${trap.fail}

Plan:
${JSON.stringify(plan, null, 2)}

Answer only with JSON: {"verdict": "PASS" or "FAIL", "reason": one sentence naming the part of the plan that decided it}.`;

// npm installs `claude` as a .cmd shim that cannot start without a shell; the native .exe it calls is the real program
// (a shim that only runs node.exe on a script is not followed).
function bin(name) {
  for (const dir of (process.env.PATH ?? '').split(path.delimiter)) {
    if (fs.existsSync(path.join(dir, `${name}.exe`))) return path.join(dir, `${name}.exe`);
    const shim = path.join(dir, `${name}.cmd`);
    const text = fs.existsSync(shim) ? fs.readFileSync(shim, 'utf8') : '';
    const exe = [...text.matchAll(/"%dp0%\\([^"]+\.exe)"/g)].map((m) => m[1]).find((e) => !/(^|\\)node\.exe$/i.test(e));
    if (exe && fs.existsSync(path.join(dir, exe))) return path.join(dir, exe);
  }
  return name;
}

function sh(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { encoding: 'utf8', maxBuffer: 1 << 30, windowsHide: true, ...opts });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')}: ${r.error ?? r.stderr}`);
  return r.stdout;
}

function run(cmd, args, { cwd, input }) {
  return new Promise((resolve) => {
    const started = Date.now();
    const child = spawn(cmd, args, { cwd, stdio: [input ? 'pipe' : 'ignore', 'pipe', 'pipe'], windowsHide: true });
    child.on('error', (e) => resolve({ code: -1, stdout, stderr: String(e), seconds: 0 }));
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => (stdout += d));
    child.stderr.on('data', (d) => (stderr += d));
    const timer = setTimeout(() => child.kill(), 20 * 60_000);
    if (input) child.stdin.end(input);
    child.on('close', (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr, seconds: Math.round((Date.now() - started) / 1000) });
    });
  });
}

function copyHead(dir, withMap) {
  fs.mkdirSync(dir, { recursive: true });
  sh('git', ['-C', repo, 'archive', '--format=tar', '-o', path.join(dir, 'head.tar'), 'HEAD']);
  sh('tar', ['-xf', 'head.tar'], { cwd: dir });
  fs.rmSync(path.join(dir, 'head.tar'));
  fs.rmSync(path.join(dir, 'testkit/coherence'), { recursive: true, force: true });
  for (const name of fs.readdirSync(path.join(dir, 'docs/research')).filter((n) => n.includes('coherence'))) {
    fs.rmSync(path.join(dir, 'docs/research', name), { recursive: true });
  }
  if (withMap) return;
  for (const file of without.delete) fs.rmSync(path.join(dir, file));
  const edit = (file, change) => {
    const p = path.join(dir, file);
    fs.writeFileSync(p, change(fs.readFileSync(p, 'utf8').replace(/\r\n/g, '\n')));
  };
  for (const [file, from, to] of without.replace) {
    edit(file, (text) => {
      if (text.split(from).length !== 2) throw new Error(`without: "${from.slice(0, 50)}" no longer appears once in ${file}`);
      return text.replace(from, () => to);
    });
  }
  for (const [file, pattern] of without.dropLines) {
    edit(file, (text) => text.split('\n').filter((line) => !new RegExp(pattern).test(line)).join('\n'));
  }
  const leak = new RegExp(without.leak);
  for (const file of fs.readdirSync(dir, { recursive: true })) {
    const rel = file.replaceAll('\\', '/');
    const p = path.join(dir, file);
    if (without.leakAllowed.some((a) => rel.startsWith(a)) || !fs.statSync(p).isFile() || fs.statSync(p).size > 2e6) continue;
    if (leak.test(fs.readFileSync(p, 'latin1'))) throw new Error(`without: ${rel} still leads to the map`);
  }
}

// The user's config stays: it holds the Windows sandbox setup that lets read-only shell commands run. Remote apps,
// plugins, hooks, subagents and every MCP server go, so the agent reads only the copy in front of it. A server a plugin
// brings has no entry of its own to turn off (the override breaks the config), and goes with its plugin or feature.
let off;
function codexOff() {
  const loads = (args) => spawnSync(bin('codex'), [...args, 'mcp', 'list', '--json'], { windowsHide: true }).status === 0;
  off ??= [
    ...['apps', 'plugins', 'remote_plugin', 'hooks', 'browser_use', 'computer_use', 'multi_agent'].flatMap((f) => ['--disable', f]),
    ...JSON.parse(sh(bin('codex'), ['mcp', 'list', '--json']))
      .filter((m) => m.enabled)
      .map((m) => ['-c', `mcp_servers.${m.name}.enabled=false`])
      .filter(loads)
      .flat(),
  ];
  return off;
}

async function plan(trap, dir, tag) {
  const prompt = ask(trap.request);
  const schema = path.join(out, 'plan.schema.json');
  if (cli === 'codex') {
    const last = path.join(out, `${tag}.last.json`);
    fs.rmSync(last, { force: true });
    const args = ['exec', ...codexOff(), '--ephemeral', '--skip-git-repo-check', '-s', 'read-only', '-m', model,
      '-c', `model_reasoning_effort="${effort}"`, '--output-schema', schema, '-o', last, '--json', prompt];
    const r = await run(bin('codex'), args, { cwd: dir });
    fs.writeFileSync(path.join(out, `${tag}.jsonl`), r.stdout);
    const usage = r.stdout.split('\n').filter((l) => l.includes('"turn.completed"')).map((l) => JSON.parse(l).usage ?? {});
    const sum = (k) => usage.reduce((n, u) => n + (u[k] ?? 0), 0);
    const tokens = { input: sum('input_tokens'), cached: sum('cached_input_tokens'), output: sum('output_tokens') };
    const [inP, cachedP, outP] = PRICES[model] ?? [NaN, NaN, NaN];
    const cost = ((tokens.input - tokens.cached) * inP + tokens.cached * cachedP + tokens.output * outP) / 1e6;
    const planned = fs.existsSync(last) ? parse(fs.readFileSync(last, 'utf8')) : null;
    return { plan: planned, tokens, cost, seconds: r.seconds, ...(planned ? {} : { error: r.stderr.slice(-500) }) };
  }
  const args = ['-p', '--model', model, '--effort', effort, '--setting-sources', 'project', '--strict-mcp-config',
    '--tools', 'Read,Grep,Glob', '--output-format', 'json', '--json-schema', JSON.stringify(PLAN), '--no-session-persistence'];
  const r = await run(bin('claude'), args, { cwd: dir, input: prompt });
  fs.writeFileSync(path.join(out, `${tag}.json`), r.stdout);
  const result = parse(r.stdout);
  const planned = result?.structured_output ?? parse(result?.result);
  return { plan: planned, tokens: result?.usage, cost: result?.total_cost_usd, seconds: r.seconds, ...(planned ? {} : { error: r.stderr.slice(-500) }) };
}

async function score(trap, planned, tag) {
  const args = ['-p', '--model', 'sonnet', '--effort', 'low', '--tools', '', '--setting-sources', 'project', '--strict-mcp-config', '--output-format', 'json',
    '--json-schema', JSON.stringify(VERDICT), '--no-session-persistence'];
  const r = await run(bin('claude'), args, { cwd: os.tmpdir(), input: judge(trap, planned) });
  fs.writeFileSync(path.join(out, `${tag}.judge.json`), r.stdout);
  const result = parse(r.stdout);
  return { ...(result?.structured_output ?? parse(result?.result) ?? { verdict: 'ERROR', reason: r.stderr.slice(0, 200) }), cost: result?.total_cost_usd };
}

function parse(text) {
  try {
    return JSON.parse(text);
  } catch {
    const m = typeof text === 'string' && text.match(/\{[\s\S]*\}/);
    try { return m ? JSON.parse(m[0]) : null; } catch { return null; }
  }
}

fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, 'plan.schema.json'), JSON.stringify(PLAN));
const file = path.join(out, 'results.json');
const results = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : [];
const key = (r) => `${r.trap}.${r.cond}.${r.rep}`;
const rejudge = 'rejudge' in flags;
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'faber-traps-'));
const dirs = { map: path.join(tmp, 'map'), nomap: path.join(tmp, 'nomap') };
if (!rejudge) {
  copyHead(dirs.map, true);
  copyHead(dirs.nomap, false);
}
if ('dry' in flags) {
  fs.rmSync(tmp, { recursive: true, force: true });
  console.log('both copies built; the map-free one leads nowhere near the map');
  process.exit(0);
}

// --rejudge scores the plans already in results.json again, with the rubrics as they are now; a new run replaces each
// row it repeats once the new one is ready, and keeps the others.
const work = rejudge
  ? results.filter((r) => r.plan && chosen.some((t) => t.id === r.trap))
  : chosen.flatMap((trap) => [...Array(reps).keys()].flatMap((rep) => ['map', 'nomap'].map((cond) => ({ trap: trap.id, rep, cond }))));
let next = 0;
async function worker() {
  while (next < work.length) {
    const item = work[next++];
    const trap = traps.find((t) => t.id === item.trap);
    const tag = key(item);
    if (rejudge) {
      const v = await score(trap, item.plan, `${tag}.again`);
      item.earlier ??= { verdict: item.verdict, reason: item.reason };
      Object.assign(item, { verdict: v.verdict, reason: v.reason, judgeCost: (item.judgeCost ?? 0) + (v.cost ?? 0) });
      console.log(`${tag}: ${item.earlier.verdict} -> ${v.verdict} ${v.reason ?? ''}`);
    } else {
      const done = await plan(trap, dirs[item.cond], tag);
      const v = done.plan ? await score(trap, done.plan, tag) : { verdict: 'ERROR', reason: 'no plan' };
      const at = results.findIndex((r) => key(r) === tag);
      const row = { ...item, model, effort, ...done, verdict: v.verdict, reason: v.reason, judgeCost: v.cost };
      if (at < 0) results.push(row);
      else results[at] = row;
      console.log(`${tag}: ${v.verdict} (${done.seconds} s, $${(done.cost ?? 0).toFixed(3)}) ${v.reason ?? ''}`);
    }
    fs.writeFileSync(file, JSON.stringify(results, null, 2));
  }
}
try {
  await Promise.all([...Array(jobs)].map(worker));
} finally {
  fs.rmSync(tmp, { recursive: true, force: true });
}

const rate = (rows) => `${rows.filter((r) => r.verdict === 'PASS').length}/${rows.length}`;
for (const id of [...new Set(results.map((r) => r.trap))]) {
  const rows = results.filter((r) => r.trap === id);
  console.log(`${id}: map ${rate(rows.filter((r) => r.cond === 'map'))}, no map ${rate(rows.filter((r) => r.cond === 'nomap'))}`);
}
console.log(`all: map ${rate(results.filter((r) => r.cond === 'map'))}, no map ${rate(results.filter((r) => r.cond === 'nomap'))}`);
const spent = results.reduce((n, r) => n + (r.cost ?? 0) + (r.judgeCost ?? 0), 0);
console.log(`spent about $${spent.toFixed(2)} API-equivalent; results in ${out}`);
