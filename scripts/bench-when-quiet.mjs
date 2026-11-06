// Release 1.0 bench: local polling only; the strict spend gate must pass before any model work.
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
const evidence = path.join(root, 'reports/reforma/helpers/g390b-evidence');
const release = '0adba5bb';
const billed = /^(ANTHROPIC_API_KEY|ANTHROPIC_AUTH_TOKEN|ANTHROPIC_BASE_URL|CLAUDE_CODE_USE_(BEDROCK|VERTEX|FOUNDRY)|OPENAI_API_KEY|CODEX_API_KEY|OPENAI_BASE_URL)$/i;

export function subscriptionEnv(env) {
  return Object.fromEntries(Object.entries(env).filter(([key]) => !billed.test(key)));
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
export async function watch({ now, pause, sample, run, record }, started = now()) {
  const deadline = started + LIMIT_MS;
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
  return '14-hour limit';
}

export function killArgs(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) throw new Error('Refusing an invalid owned child PID');
  return ['/PID', String(pid), '/T', '/F'];
}

export function verifiedQuota(receipt) {
  return Array.isArray(receipt?.readings) && receipt.readings.length === 2 && ['claude', 'codex'].every((vendor) =>
    receipt.readings.filter((r) => r.vendor === vendor).length === 1 && receipt.readings.some((r) =>
      r.vendor === vendor && !r.unknownCost && !r.error && typeof r.source === 'string' &&
      Array.isArray(r.windows) && r.windows.length > 0 && r.windows.every((w) =>
        typeof w.window === 'string' && Number.isFinite(w.used) && w.used >= 0 && w.used <= 1.5 &&
        Number.isFinite(w.resetsAt) && w.resetsAt > r.completedAt)));
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
  if (process.platform !== 'win32') throw new Error('This release watcher requires Windows');
  await mkdir(evidence, { recursive: true });
  const lock = await open(path.join(evidence, 'watcher.lock'), 'wx');
  await lock.writeFile(String(process.pid));
  const events = createWriteStream(path.join(evidence, 'watcher.jsonl'), { flags: 'a' });
  const record = (value) => { const entry = { time: new Date().toISOString(), ...value }; events.write(`${JSON.stringify(entry)}\n`); console.log(JSON.stringify(entry)); };
  let active = null;
  let stopping = null;
  let gameAt = 0;
  let interrupted = false;
  const killOwned = () => {
    if (!active || active.exitCode !== null || active.signalCode !== null) return;
    // taskkill sees only the PID returned by our own spawn; never /IM or another session's process.
    execFileSync('taskkill.exe', killArgs(active.pid), { windowsHide: true, timeout: 15_000, stdio: 'ignore' });
  };
  const onGame = (reason = 'game') => {
    gameAt = Date.now();
    if (active) {
      stopping = reason;
      try { killOwned(); } catch { stopping = 'owned process tree termination failed'; }
    }
    record({ event: reason === 'game' ? 'cs2-start' : reason, ownedPid: active?.pid ?? null });
  };
  let guard;
  const receipts = [];
  const attempts = [];
  let quotaUnknown = false;
  const started = Date.now();
  let result = 'not started';
  const stop = () => { interrupted = true; stopping = 'interrupted'; try { killOwned(); } catch { /* reported by the final receipt */ } };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
  try {
    const git = (args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
    git(['merge-base', '--is-ancestor', release, 'HEAD']);
    const productDiff = git(['diff', release, '--', 'engine', 'host', 'ui', 'winproc', 'channel', 'sidecar', 'Cargo.toml', 'Cargo.lock',
      '.cargo', 'build.rs', ':!engine/src/account.rs', ':!engine/examples/bench.rs']);
    // The only product-source edit permitted here exposes the existing local quota reader to the harness.
    const accountSource = await readFile(path.join(root, 'engine/src/account.rs'), 'utf8');
    const releaseAccount = git(['show', `${release}:engine/src/account.rs`]);
    if (productDiff || accountSource.replace('pub async fn claude_limits(', 'pub(crate) async fn claude_limits(').replaceAll('\r', '').trim() !== releaseAccount.replaceAll('\r', ''))
      throw new Error('Product differs from the release candidate');
    await writeFile(path.join(evidence, 'provenance.json'), JSON.stringify({ release: git(['rev-parse', release]), head: git(['rev-parse', 'HEAD']), productDiff }, null, 2));
    const env = subscriptionEnv({ ...process.env, RUSTUP_HOME: 'E:\\Apps\\rust\\rustup', CARGO_HOME: 'E:\\Apps\\rust\\cargo',
      PATH: `E:\\Apps\\zig;E:\\Apps\\rust\\cargo\\bin;${process.env.PATH ?? ''}` });
    os.setPriority(process.pid, os.constants.priority.PRIORITY_BELOW_NORMAL);
    guard = await gameGuard(onGame);
    const invoke = async (args, name, deadline) => {
      guard.check();
      if (interrupted) return { reason: 'interrupted' };
      if (Date.now() >= deadline) return { reason: '14-hour limit' };
      if (gameRunning()) return { reason: 'game' };
      stopping = null;
      const out = createWriteStream(path.join(evidence, `${name}.stdout.log`));
      const err = createWriteStream(path.join(evidence, `${name}.stderr.log`));
      const child = spawn(process.execPath, ['scripts/cargo.mjs', 'run', '-p', 'faber-engine', '--example', 'bench', '--', ...args],
        { cwd: root, env, windowsHide: true, shell: false, stdio: ['ignore', 'pipe', 'pipe'] });
      active = child;
      child.stdout.pipe(out);
      child.stderr.pipe(err);
      record({ event: 'started', name, pid: child.pid });
      const timer = setTimeout(() => { stopping = '14-hour limit'; try { killOwned(); } catch { stopping = 'owned process tree termination failed'; } }, Math.max(1, deadline - Date.now()));
      const outcome = await new Promise((resolve) => {
        child.once('error', (error) => resolve({ reason: `spawn failed: ${error.code}` }));
        child.once('close', (code, signal) => resolve({ code, signal, reason: stopping }));
      });
      clearTimeout(timer);
      active = null;
      await Promise.all([new Promise((resolve) => out.end(resolve)), new Promise((resolve) => err.end(resolve))]);
      guard.check();
      record({ event: 'ended', name, ...outcome });
      return outcome;
    };
    let previousCpu = cpuSnapshot();
    result = await watch({
      now: Date.now,
      pause: async (ms) => { await delay(ms); guard.check(); if (interrupted) throw new Error('interrupted'); },
      sample: async () => {
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
    record({ event: 'failed', error: result });
  } finally {
    guard?.close();
    process.removeListener('SIGINT', stop);
    process.removeListener('SIGTERM', stop);
    const summary = { result, started: new Date(started).toISOString(), ended: new Date().toISOString(), capUsd: CAP_USD, attempts, receipts };
    await writeFile(path.join(evidence, 'summary.json'), JSON.stringify(summary, null, 2));
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
