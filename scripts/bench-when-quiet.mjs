// Release 1.0 bench: local polling, then the selected quota/spend gate before model work.
import { spawn, execFileSync } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { mkdir, open, readFile, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

export const POLL_MS = 60_000;
export const QUIET_MS = 10 * POLL_MS;
export const LIMIT_MS = 14 * 60 * POLL_MS;
export const CAP_USD = 25;
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// The engine correctly refuses workspaces under credential-folder names, including this .claude worktree.
export const CODEX_EVIDENCE = path.join(os.tmpdir(), 'faber-g390c2-final-evidence');
export const CLAUDE_EVIDENCE = path.join(os.tmpdir(), 'faber-g390d-evidence');
export const CLAUDE_RESET = Date.parse('2026-11-10T08:00:00Z');
export const CLAUDE_DEADLINE = Date.parse('2026-11-12T00:00:00Z');
const requests = ['organize-folder', 'plan-an-app', 'todo-app', 'research-with-sources', 'everyday-question', 'fix-invoice', 'migrate-settings', 'summarize-sales'];
const billed = /API_KEY|AUTH_TOKEN|ACCESS_TOKEN|BEARER_TOKEN|SECRET_ACCESS_KEY|ACCESS_KEY_ID|SESSION_TOKEN|BILLING|BASE_URL|^OPENAI_(ORG_ID|PROJECT_ID|ORGANIZATION|PROJECT)$|^CLAUDE_CODE_USE_|^AWS_PROFILE$|^GOOGLE_APPLICATION_CREDENTIALS$/i;

export function codexWeekly(receipt) {
  if (!verifiedQuota(receipt, ['codex'])) throw new Error('Codex quota receipt missing or invalid');
  const windows = receipt.readings[0].windows.filter((w) => w.window === 'seven_day');
  if (windows.length !== 1) throw new Error('Codex weekly quota missing or duplicated');
  return windows[0].used;
}

export function watcherOptions(args) {
  const codexOnly = args.includes('--codex-only');
  const claudeOnly = args.includes('--claude-only');
  if (codexOnly && claudeOnly) throw new Error('--claude-only and --codex-only are mutually exclusive');
  let notBefore = 0;
  let scheduled = false;
  for (let i = 0; i < args.length; i++) {
    if (['--codex-only', '--claude-only'].includes(args[i])) continue;
    if (args[i] !== '--not-before' || scheduled) throw new Error(`Unknown or duplicate option: ${args[i]}`);
    scheduled = true;
    const value = args[++i];
    if (!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{3})?Z$/.test(value ?? '') || !Number.isFinite(Date.parse(value)) ||
        new Date(Date.parse(value)).toISOString() !== value.replace(/Z$/, value.includes('.') ? 'Z' : '.000Z'))
      throw new Error('--not-before requires a valid UTC ISO time');
    notBefore = Date.parse(value);
  }
  if (claudeOnly) notBefore = Math.max(notBefore, CLAUDE_RESET);
  if (claudeOnly && notBefore >= CLAUDE_DEADLINE) throw new Error('--not-before must precede the Claude deadline');
  return { codexOnly, claudeOnly, notBefore };
}

export function claudeWeekly(receipt, now) {
  if (!verifiedQuota(receipt, ['claude'])) throw new Error('Claude quota receipt missing or invalid; no work admitted (bench/account_quiet.rs:39)');
  const reading = receipt.readings[0];
  if (reading.paidFallback !== false) throw new Error('Claude paid extra usage enabled or unknown; subscription-only admission refused');
  const windows = reading.windows.filter((w) => w.window === 'seven_day');
  if (reading.source !== 'app claude_limits (control-only get_usage, zero model cost)' || windows.length !== 1 ||
      !Number.isFinite(reading.completedAt) || reading.completedAt * 1000 < CLAUDE_RESET || reading.completedAt * 1000 > now ||
      windows[0].resetsAt * 1000 <= now || windows[0].resetsAt * 1000 > CLAUDE_RESET + 7 * 24 * 60 * 60_000)
    throw new Error('Claude new-week quota missing, stale or duplicated; no work admitted (bench/account_quiet.rs:39)');
  return windows[0].used;
}

export function subscriptionEnv(env) {
  const child = Object.fromEntries(Object.entries(env).filter(([key]) => !billed.test(key)));
  for (const key of Object.keys(child)) if (key.toUpperCase() === 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC') {
    // The CLI treats quota traffic as nonessential; keep its telemetry opt-out without blocking the usage endpoint.
    delete child[key];
    child.DISABLE_TELEMETRY = '1';
    child.DISABLE_ERROR_REPORTING = '1';
  }
  return child;
}

export function cpuSnapshot(cpus = os.cpus()) {
  return cpus.reduce((sum, { times }) => ({
    idle: sum.idle + times.idle,
    total: sum.total + Object.values(times).reduce((a, b) => a + b, 0),
  }), { idle: 0, total: 0 });
}

export function cpuPercent(previous, current) {
  const total = current.total - previous.total;
  const idle = current.idle - previous.idle;
  return total > 0 && idle >= 0 && idle <= total ? 100 * (1 - idle / total) : NaN;
}

export function quietSince(since, sample, previousAt) {
  if (sample.game || !Number.isFinite(sample.cpu) || sample.cpu < 0 || sample.cpu >= 15 ||
      sample.at <= previousAt || sample.at - previousAt > 1.5 * POLL_MS || sample.gameAt > previousAt) return null;
  return since ?? previousAt;
}

// Injected clock/I/O keep the 14-hour policy and retry checks free of real processes and subscriptions.
export async function watch({ now, pause, sample, run, record, notBefore = 0, deadline: end }, started = now()) {
  const deadline = end ?? started + LIMIT_MS;
  const limit = end === undefined ? '14-hour limit' : 'deadline';
  while (now() < notBefore && now() < deadline) {
    record({ event: 'waiting-for-reset', notBefore: new Date(notBefore).toISOString() });
    await pause(Math.min(POLL_MS, notBefore - now(), deadline - now()));
  }
  if (now() >= deadline) return limit;
  let since = null;
  const initial = await sample();
  let previousAt = initial.at;
  let previousGame = initial.game;
  while (now() < deadline) {
    await pause(Math.min(POLL_MS, deadline - now()));
    if (now() >= deadline) break;
    const state = await sample();
    since = previousGame ? null : quietSince(since, state, previousAt);
    previousAt = state.at;
    previousGame = state.game;
    record({ ...state, quietMs: since === null ? 0 : state.at - since });
    if (since !== null && state.at - since >= QUIET_MS) {
      const result = await run(deadline);
      if (result !== 'game') return result;
      since = null;
      const fresh = await sample();
      previousAt = fresh.at;
      previousGame = fresh.game;
    }
  }
  return limit;
}

export function killArgs(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) throw new Error('Refusing an invalid owned child PID');
  return ['/PID', String(pid), '/T', '/F'];
}

export function verifiedQuota(receipt, vendors = ['claude', 'codex']) {
  return Array.isArray(receipt?.readings) && receipt.readings.length === vendors.length && vendors.every((vendor) =>
    receipt.readings.filter((r) => r.vendor === vendor).length === 1 && receipt.readings.some((r) =>
      r.vendor === vendor && !r.unknownCost && !r.error && typeof r.source === 'string' &&
      Array.isArray(r.windows) && r.windows.length > 0 && r.windows.every((w) =>
        typeof w.window === 'string' && Number.isFinite(w.used) && w.used >= 0 && w.used <= 1.5 &&
        Number.isFinite(w.resetsAt) && w.resetsAt > r.completedAt)));
}

// Repeated Claude usage reads within minutes can come back without the per-model rows the reader requires
// (engine/src/account.rs usage_events) and are refused as unreceipted; that one reading is retried after a pause.
export const QUOTA_RETRIES = 3;
export const QUOTA_RETRY_MS = 3 * 60_000;
const unreceipted = (receipt) => receipt?.readings?.length === 1 && /no verified local quota receipt/.test(receipt.readings[0].error ?? '');
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// The same per-request sequence serves both subscription modes; I/O is injectable for quota-free proofs.
export async function subscriptionRequests({ claudeOnly, attempt, evidence, invoke, receipts, record, comparisons = [], now = Date.now, read = readFile, wait = pause }, deadline) {
  const vendor = claudeOnly ? 'claude' : 'codex';
  for (const request of requests) {
    let benchOutcome;
    for (const phase of ['before', 'bench', 'after']) {
      const name = `attempt-${attempt}-${request}-${phase}`;
      const quota = phase !== 'bench';
      const args = quota ? ['--quota-only', `--${vendor}-only`] : [`--${vendor}-only`, '--only', request,
        '--cap-usd', String(CAP_USD), '--per-run-usd', String(CAP_USD), '--dir', evidence, '--run-id', name];
      let outcome, receipt;
      for (let tries = 0; ; tries++) {
        const run = tries ? `${name}-retry-${tries}` : name;
        outcome = await invoke(args, run, deadline);
        if (!quota || outcome.reason) break;
        const output = await read(path.join(evidence, `${run}.stdout.log`), 'utf8');
        receipt = JSON.parse(output.trim().split(/\r?\n/).findLast((line) => line.startsWith('{"readings":')) ?? 'null');
        receipts.push({ attempt, request, phase, ...(tries ? { retry: tries } : {}), ...receipt });
        if (!claudeOnly || tries >= QUOTA_RETRIES || !unreceipted(receipt)) break;
        record({ event: 'quota-retry', request, phase, retry: tries + 1 });
        await wait(QUOTA_RETRY_MS);
      }
      if (!quota) comparisons.push({ request, ...outcome });
      if (outcome.reason) return outcome.reason === 'game' ? 'stopped: cs2 started' : outcome.reason;
      if (quota) {
        const used = claudeOnly ? claudeWeekly(receipt, now()) : codexWeekly(receipt);
        record({ event: 'weekly-quota', request, phase, used });
        if (outcome.code !== 0) return `${vendor} quota reader exited ${outcome.code}`;
        const threshold = claudeOnly ? 0.25 : 0.78;
        if (used > threshold) return `stopped: ${vendor} weekly ${(used * 100).toFixed(1)}% used exceeds ${threshold * 100}%`;
      } else {
        benchOutcome = outcome;
      }
    }
    if (benchOutcome.code !== 0) return `${request} benchmark exited ${benchOutcome.code}; see stderr log`;
    const saved = JSON.parse(await read(path.join(evidence, `attempt-${attempt}-${request}-bench`, 'results.json'), 'utf8'));
    if (!saved.rows?.some((r) => r.plain && r.faber)) return `${request}: no comparison result`;
  }
  return 'finished';
}

export function claudeReport(summary, results, evidence) {
  const cell = (value) => String(value).replaceAll('|', '\\|').replaceAll(/\r?\n/g, ' ');
  const tokens = (usage) => usage ? ['input', 'output', 'cache_read', 'cache_write'].reduce((n, key) => n + (usage[key] ?? 0), 0) : 'unknown';
  const quota = (request, phase) => {
    const r = summary.receipts.findLast((r) => r.request === request && r.phase === phase)?.readings?.[0];
    if (!r) return 'not read';
    const w = r.windows?.find((w) => w.window === 'seven_day');
    return w && Number.isFinite(w.used) && Number.isFinite(w.resetsAt) && Number.isFinite(r.completedAt) ?
      `${(w.used * 100).toFixed(1)}%; read ${new Date(r.completedAt * 1000).toISOString()}; resets ${new Date(w.resetsAt * 1000).toISOString()}; ${r.source}` :
      `unavailable: ${r.error ?? 'no weekly window'}`;
  };
  const lines = requests.map((request) => {
    const row = results.flatMap((r) => r.rows ?? []).find((r) => r.request === request);
    const run = summary.comparisons.find((r) => r.request === request);
    const verdict = row?.verdict ? `${row.winner ?? 'tie'}: ${row.verdict.reason}` : row?.skipped ?? (run ? 'no verdict; interrupted or incomplete' : 'not started');
    const proof = (side) => side?.proof ? (side.proof.pass ? 'pass' : 'fail') : 'unavailable';
    return `| ${[request, verdict, `${proof(row?.plain)} / ${proof(row?.faber)}`,
      row ? `${tokens(row.plain?.usage)} / ${tokens(row.faber?.usage)} / ${row.judge_attempted ? tokens(row.judge_usage) : 'not started'}` : run ? 'unknown' : 'not started',
      row ? `${row.plain?.wall_secs ?? 'unknown'} / ${row.faber?.wall_secs ?? 'unknown'}` : run ? 'unknown' : 'not started',
      run?.secs ?? (run ? 'unknown' : 'not started'), quota(request, 'before'), quota(request, 'after')].map(cell).join(' | ')} |`;
  });
  return `# g390d: scheduled Claude eight-request benchmark

Status: ${summary.result === 'finished' ? 'execution finished; inspect verdict coverage below' : 'stopped without full qualification'}.
Stop reason: ${cell(summary.result)}.
Watcher: ${summary.started} to ${summary.ended}. Not before: ${summary.notBefore}. Deadline: ${summary.deadline}.

| Request | Verdict / reason | Proof P / F | Reported tokens P / F / J | Wall seconds P / F | Comparison seconds | Claude weekly before | Claude weekly after |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
${lines.join('\n')}

P = plain Claude; F = Faber; J = Claude judge. Token sums include input, output and cache reads/writes. Faber routing/auxiliary totals are partial; missing results from interrupted work are unknown, never zero.
Quota observations are unpaired, rounded/delayed, and shared with the owner; they cannot attribute consumption. Admission stops strictly above 25%, on a missing/invalid new-week receipt, CS2 start during an attempt, or the deadline. The threshold is checked before and after each comparison, including its judge; in-flight consumption cannot be observed between those readings. CS2 detection is 250 ms polling plus OS termination latency.
Subscription-only: no dollar ceiling, billing/API environment names stripped before every watcher child. The quota source is the fixed control-only app reader; no text-command fallback. Runtime evidence: ${evidence} (summary.json, provenance.json, watcher.jsonl, per-phase logs and per-request results.json).
Blocker location: ${summary.blocker ?? 'none (completed run or policy stop)'}. No live game-start test or heavy Rust/UI suites were run. No release-wide independent-quality or token-saving claim.
`;
}

const guardCommand = `
$ErrorActionPreference = 'Stop'
[Diagnostics.Process]::GetCurrentProcess().PriorityClass = 'BelowNormal'
'READY'
$seen = $false
while ($true) {
  $games = [Diagnostics.Process]::GetProcessesByName('cs2')
  $running = $games.Length -gt 0
  if ($running -and -not $seen) { 'GAME' }
  $seen = $running
  foreach ($game in $games) { $game.Dispose() }
  [Threading.Thread]::Sleep(250)
}
`;

export async function gameGuard(onGame) {
  const child = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(guardCommand, 'utf16le').toString('base64')],
    { cwd: root, windowsHide: true, shell: false, stdio: ['ignore', 'pipe', 'pipe'] });
  let ready = false;
  let closed = false;
  let failure = null;
  let diagnostics = '';
  let pending = '';
  const initialized = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Game-start guard did not become ready')), 15_000);
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      pending += chunk;
      const lines = pending.split(/\r?\n/);
      pending = lines.pop();
      for (const line of lines) {
        if (line.trim() === 'READY') { ready = true; clearTimeout(timer); resolve(); }
        if (line.trim() === 'GAME' && !closed) onGame();
      }
    });
    // Only our fixed guard script can print here; discard OS diagnostics rather than save machine details.
    child.stderr.on('data', (chunk) => { diagnostics = (diagnostics + chunk).slice(-2000); });
    child.once('error', () => { clearTimeout(timer); reject(new Error('Cannot start game-start guard')); });
    child.once('close', () => {
      clearTimeout(timer);
      if (!closed) {
        failure = new Error(`Game-start guard ended; no work is safe to admit: ${diagnostics.trim()}`);
        if (ready) onGame('guard-failed');
        if (!ready) reject(failure);
      }
    });
  });
  try { await initialized; } catch (error) { closed = true; child.kill(); throw error; }
  return {
    check() { if (failure) throw failure; },
    close() { closed = true; child.kill(); },
  };
}

function gameRunning() {
  return /"cs2\.exe"/i.test(execFileSync('tasklist.exe', ['/FI', 'IMAGENAME eq cs2.exe', '/FO', 'CSV', '/NH'],
    { encoding: 'utf8', windowsHide: true, timeout: 10_000 }));
}

async function main() {
  const { codexOnly, claudeOnly, notBefore } = watcherOptions(process.argv.slice(2));
  const subscriptionOnly = codexOnly || claudeOnly;
  // A re-run after product fixes names its own candidate; the default stays the first measured one.
  const release = process.env.FABER_BENCH_RELEASE || '4bc453277a64e9b7ca26835c1df7ecc128d52e5f';
  const evidence = claudeOnly ? CLAUDE_EVIDENCE : codexOnly ? CODEX_EVIDENCE : path.join(root, 'reports/reforma/helpers/g390b-evidence');
  const deadline = claudeOnly ? CLAUDE_DEADLINE : undefined;
  const limit = claudeOnly ? 'deadline' : '14-hour limit';
  if (process.platform !== 'win32') throw new Error('This release watcher requires Windows');
  // Sanitize before git, the guard, tasklist and taskkill too; descendants inherit this environment.
  for (const key of Object.keys(process.env)) if (billed.test(key)) delete process.env[key];
  await mkdir(evidence, { recursive: true });
  const lock = await open(path.join(evidence, 'watcher.lock'), 'wx');
  await lock.writeFile(String(process.pid));
  const events = createWriteStream(path.join(evidence, 'watcher.jsonl'), { flags: 'a' });
  const record = (value) => { const entry = { time: new Date().toISOString(), ...value }; events.write(`${JSON.stringify(entry)}\n`); console.log(JSON.stringify(entry)); };
  let active = null;
  let stopping = null;
  let gameAt = 0;
  let interrupted = false;
  let subscriptionStopped = false;
  const killOwned = () => {
    if (!active || active.exitCode !== null || active.signalCode !== null) return;
    // taskkill sees only the PID returned by our own spawn; never /IM or another session's process.
    execFileSync('taskkill.exe', killArgs(active.pid), { windowsHide: true, timeout: 15_000, stdio: 'ignore' });
  };
  const onGame = (reason = 'game') => {
    gameAt = Date.now();
    if (subscriptionOnly && attempts.length > 0) subscriptionStopped = true;
    if (active) {
      stopping = reason;
      try { killOwned(); } catch { stopping = 'owned process tree termination failed'; }
    }
    record({ event: reason === 'game' ? 'cs2-start' : reason, ownedPid: active?.pid ?? null });
  };
  let guard;
  const receipts = [];
  const attempts = [];
  const comparisons = [];
  let quotaUnknown = false;
  const started = Date.now();
  let result = 'not started';
  let blocker = null;
  const stop = () => { interrupted = true; stopping = 'interrupted'; try { killOwned(); } catch { /* reported by the final receipt */ } };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
  try {
    const git = (args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
    git(['merge-base', '--is-ancestor', release, 'HEAD']);
    const productDiff = git(['diff', release, '--', 'engine', 'host', 'ui', 'winproc', 'channel', 'sidecar', 'Cargo.toml', 'Cargo.lock',
      '.cargo', 'build.rs', ':!engine/src/account.rs', ':!engine/examples/bench.rs']);
    // The requested base merge advances provenance for all modes without changing their admission policies.
    const accountSource = await readFile(path.join(root, 'engine/src/account.rs'), 'utf8');
    const releaseAccount = git(['show', `${release}:engine/src/account.rs`]);
    if (productDiff || accountSource.replace('pub async fn claude_limits(', 'pub(crate) async fn claude_limits(').replaceAll('\r', '').trim() !== releaseAccount.replaceAll('\r', ''))
      throw new Error('Product differs from the release candidate');
    await writeFile(path.join(evidence, 'provenance.json'), JSON.stringify({ release: git(['rev-parse', release]), head: git(['rev-parse', 'HEAD']), productDiff }, null, 2));
    const env = subscriptionEnv({ ...process.env, RUSTUP_HOME: 'E:\\Apps\\rust\\rustup', CARGO_HOME: 'E:\\Apps\\rust\\cargo',
      PATH: `E:\\Apps\\zig;E:\\Apps\\rust\\cargo\\bin;${process.env.PATH ?? ''}` });
    os.setPriority(process.pid, os.constants.priority.PRIORITY_BELOW_NORMAL);
    const invoke = async (args, name, deadline) => {
      guard.check();
      if (subscriptionStopped) return { reason: 'stopped: cs2 started during the admitted attempt' };
      if (interrupted) return { reason: 'interrupted' };
      if (Date.now() >= deadline) return { reason: limit };
      if (gameRunning()) return { reason: 'game' };
      stopping = null;
      const out = createWriteStream(path.join(evidence, `${name}.stdout.log`));
      const err = createWriteStream(path.join(evidence, `${name}.stderr.log`));
      const child = spawn(process.execPath, ['scripts/cargo.mjs', 'run', '-p', 'faber-engine', '--example', 'bench', '--', ...args],
        { cwd: root, env, windowsHide: true, shell: false, stdio: ['ignore', 'pipe', 'pipe'] });
      active = child;
      const invokedAt = Date.now();
      child.stdout.pipe(out);
      child.stderr.pipe(err);
      record({ event: 'started', name, pid: child.pid });
      const timer = setTimeout(() => { stopping = limit; try { killOwned(); } catch { stopping = 'owned process tree termination failed'; } }, Math.max(1, deadline - Date.now()));
      const outcome = await new Promise((resolve) => {
        child.once('error', (error) => resolve({ reason: `spawn failed: ${error.code}` }));
        child.once('close', (code, signal) => resolve({ code, signal, reason: stopping }));
      });
      clearTimeout(timer);
      active = null;
      await Promise.all([new Promise((resolve) => out.end(resolve)), new Promise((resolve) => err.end(resolve))]);
      guard.check();
      record({ event: 'ended', name, ...outcome });
      return { ...outcome, secs: (Date.now() - invokedAt) / 1000 };
    };
    let previousCpu = cpuSnapshot();
    result = await watch({
      now: Date.now,
      notBefore,
      deadline,
      pause: async (ms) => { await delay(ms); guard?.check(); if (interrupted) throw new Error('interrupted'); },
      sample: async () => {
        guard ??= await gameGuard(onGame);
        guard.check();
        const cpu = cpuSnapshot();
        const state = { at: Date.now(), cpu: cpuPercent(previousCpu, cpu), game: gameRunning(), gameAt };
        previousCpu = cpu;
        return state;
      },
      record,
      run: async (deadline) => {
        const attempt = attempts.length + 1;
        attempts.push({ attempt });
        if (subscriptionOnly) {
          return subscriptionRequests({ claudeOnly, attempt, evidence, invoke, receipts, comparisons, record }, deadline);
        }
        let failure = quotaUnknown ? 'quota spending unverified; no more readings' : null;
        for (const phase of ['before', 'bench-claude', 'bench-codex', 'after']) {
          const name = `attempt-${attempt}-${phase}`;
          const quota = phase === 'before' || phase === 'after';
          if (quota && quotaUnknown) { record({ event: 'skipped', phase, reason: 'quota spending unknown; no more readings' }); continue; }
          const vendor = phase.replace('bench-', '');
          const args = quota ? ['--quota-only'] : ['--require-hard-cap', '--plain', vendor, '--vendors', 'claude,codex',
            '--cap-usd', String(CAP_USD / 2), '--dir', evidence, '--run-id', name];
          const outcome = await invoke(args, name, deadline);
          if (outcome.reason) { quotaUnknown ||= quota; previousCpu = cpuSnapshot(); return outcome.reason; }
          if (quota) {
            const text = await readFile(path.join(evidence, `${name}.stdout.log`), 'utf8');
            try {
              const receipt = JSON.parse(text.trim().split(/\r?\n/).findLast((line) => line.startsWith('{"readings":')) ?? 'null');
              if (!Array.isArray(receipt.readings)) throw new Error('missing readings');
              receipts.push({ attempt, phase, ...receipt });
              quotaUnknown = !verifiedQuota(receipt) || outcome.code !== 0;
              if (quotaUnknown) failure = 'quota spending unknown; no more readings; only hard-cap refusal receipts collected';
            } catch { quotaUnknown = true; failure = `quota receipt missing or invalid: ${name}`; }
          }
          if (outcome.code !== 0) failure = `${phase} exited ${outcome.code}; see ${name}.stderr.log`;
        }
        return failure ?? 'finished';
      },
    }, started);
  } catch (error) {
    result = error.message;
    const line = error.stack?.match(/bench-when-quiet\.mjs:(\d+)/)?.[1];
    blocker = line ? `scripts/bench-when-quiet.mjs:${line}` : null;
    record({ event: 'failed', error: result });
  } finally {
    guard?.close();
    process.removeListener('SIGINT', stop);
    process.removeListener('SIGTERM', stop);
    const summary = { result, blocker, started: new Date(started).toISOString(), ended: new Date().toISOString(), codexOnly, claudeOnly,
      notBefore: notBefore ? new Date(notBefore).toISOString() : null, deadline: deadline ? new Date(deadline).toISOString() : null,
      capUsd: subscriptionOnly ? null : CAP_USD, weeklyStopUsed: claudeOnly ? 0.25 : codexOnly ? 0.78 : null, attempts, receipts, comparisons };
    await writeFile(path.join(evidence, 'summary.json'), JSON.stringify(summary, null, 2));
    if (claudeOnly) {
      const results = [];
      for (const { attempt } of attempts) for (const request of requests) {
        try { results.push(JSON.parse(await readFile(path.join(evidence, `attempt-${attempt}-${request}-bench`, 'results.json'), 'utf8'))); }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
      }
      await writeFile(path.join(root, 'reports/reforma/helpers/g390d.result.md'), claudeReport(summary, results, evidence));
      await writeFile(path.join(root, 'reports/reforma/helpers/g390d.done'), '');
    }
    record({ event: 'finished', result });
    await new Promise((resolve) => events.end(resolve));
    await lock.close();
    // Keep the lock receipt: a second invocation cannot discard the shared budget or overwrite evidence.
  }
  process.exitCode = result === 'finished' ? 0 : 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
