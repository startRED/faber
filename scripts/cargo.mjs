// Share a bounded cache pool across checkouts, while keeping each invocation's temp private.
import { spawn, execFile, execFileSync } from 'node:child_process';
import { mkdir, lstat, readFile, writeFile, rename, rm, rmdir, realpath, readdir, readlink, symlink, unlink, utimes } from 'node:fs/promises';
import path from 'node:path';
import { realpathSync } from 'node:fs';
import os from 'node:os';
import { createHash, randomUUID } from 'node:crypto';
import { setTimeout } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const DAY = 86400000;
const REAP_STALE = 60000;
// The native call, like fs/promises realpath, expands Windows 8.3 names (RUNNER~1), so recorded paths compare equal.
const systemTemp = realpathSync.native(os.tmpdir());
const tempNamespace = path.join(systemTemp, tempNamespaceName(process.platform, process.getuid?.()));

export function tempNamespaceName(platform, uid) {
  return platform === 'win32' ? 'faber-cargo' : `faber-cargo-${uid}`;
}

export function validateTempNamespace(stat, uid, folder) {
  if (!stat.isDirectory() || stat.isSymbolicLink() || stat.uid !== uid || (stat.mode & 0o022) !== 0) {
    throw new Error(`Refusing unsafe build temporary namespace: ${folder}; expected a real directory owned by uid ${uid} without group or other write access`);
  }
}

export function validateTempAncestor(stat, uid, folder) {
  if (!stat.isDirectory() || stat.isSymbolicLink() || (stat.uid !== 0 && stat.uid !== uid) || ((stat.mode & 0o022) !== 0 && (stat.mode & 0o1000) === 0)) {
    throw new Error(`Refusing unsafe build temporary ancestor: ${folder}; expected a real directory owned by root or uid ${uid}, with the sticky bit if group or other writable`);
  }
}

function tempParent(target) {
  return path.join(tempNamespace, createHash('sha256').update(process.platform === 'win32' ? target.toLowerCase() : target).digest('hex').slice(0, 32));
}
const execute = promisify(execFile);

async function directory(folder) {
  const parent = path.dirname(folder);
  if (parent !== folder) await directory(parent);
  let stat;
  try { stat = await lstat(folder); } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    try { await mkdir(folder, { mode: folder === tempNamespace ? 0o700 : 0o777 }); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    stat = await lstat(folder);
  }
  if (folder === tempNamespace && process.platform !== 'win32') {
    const uid = process.getuid();
    validateTempNamespace(stat, uid, folder);
    let ancestor = folder;
    for (;;) {
      validateTempAncestor(stat, uid, ancestor);
      const parent = path.dirname(ancestor);
      if (parent === ancestor) break;
      ancestor = parent;
      stat = await lstat(ancestor);
    }
  }
  else if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error(`Expected a real directory, not a link: ${folder}`);
}

function dead(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try { process.kill(pid, 0); return false; } catch (error) { return error.code === 'ESRCH'; }
}

// A dead child proves its whole tree is gone when it ran in a kill-on-close job (Windows), or when its process group,
// which outlives the leader while any member runs, no longer exists (other systems). shortcut: a process that left its
// group with setsid is not seen; the day guard still applies to receipts without proof.
function treeGone(owner) {
  if (!Number.isSafeInteger(owner.childPid) || owner.childPid <= 0) return false;
  if (owner.contained === 'job') return process.platform === 'win32';
  if (owner.contained !== 'group' || process.platform === 'win32') return false;
  try { process.kill(-owner.childPid, 0); return false; } catch (error) { return error.code === 'ESRCH'; }
}

async function removeTemp(parent, name) {
  if (!/^run-[a-zA-Z0-9-]+$/.test(name)) throw new Error('Invalid owned temporary directory');
  await directory(parent);
  const folder = path.join(parent, name);
  try {
    const stat = await lstat(folder);
    if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error(`Refusing temporary directory link: ${folder}`);
    // rm unlinks links inside this directory; it never traverses their destinations.
    await rm(folder, { recursive: true });
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
}

async function reap(target) {
  const lease = path.join(target, '.faber-lease');
  const lock = path.join(target, '.faber-reap');
  try {
    await mkdir(lock);
    await writeFile(path.join(lock, 'owner'), String(process.pid));
  } catch (error) {
    // A reaper killed mid-way left its lock, which held the cache forever (a lane waited 3.5 hours on 2026-11-11). A
    // lock whose owner is dead, or that never got an owner within a minute, is abandoned; a live, paused reaper keeps
    // it. The lease it guards is still checked as before.
    if (error.code === 'EEXIST' && await abandoned(lock, REAP_STALE)) {
      await takeOver(lock);
      return false;
    }
    // An inaccessible reaper lock never authorizes touching an occupied cache.
    if (['EEXIST', 'EPERM', 'EACCES', 'EBUSY'].includes(error.code)) return false;
    throw error;
  }
  try {
    const stat = await lstat(lease);
    if (!stat.isDirectory() || stat.isSymbolicLink()) return;
    const receipt = path.join(lease, 'owner.json');
    if (!(await lstat(receipt)).isFile() || (await lstat(receipt)).isSymbolicLink()) return;
    const owner = JSON.parse(await readFile(receipt, 'utf8'));
    if (!Number.isFinite(owner.createdAt) || !dead(owner.pid) || !dead(owner.childPid)) return;
    // Without proof that no descendant survives, an orphaned compiler may still write the cache, so only a day settles it.
    if ((Date.now() - stat.mtimeMs < DAY || Date.now() - owner.createdAt < DAY) && !treeGone(owner)) return;
    if (await overlapsLease(target)) return;
    if (typeof owner.tempName !== 'string' || !/^run-[a-zA-Z0-9-]+$/.test(owner.tempName)) return;
    const legacy = path.join(target, '.faber-tmp');
    if (owner.tempPath !== undefined) {
      let canonical;
      try {
        if (owner.tempPath === path.join(tempParent(target), owner.tempName) || owner.tempPath === path.join(legacy, owner.tempName)) {
          await directory(tempNamespace);
          canonical = await realpath(owner.tempPath);
        }
      } catch (error) { if (error.code !== 'ENOENT') throw error; canonical = null; }
      if (canonical !== null) {
        if (canonical === owner.tempPath) {
          await removeTemp(path.dirname(canonical), owner.tempName);
        } else console.error(`Build temporary directory was left for system cleanup: ${JSON.stringify(owner.tempPath)}`);
      }
    }
    try {
      await lstat(legacy);
      await removeTemp(legacy, owner.tempName);
      await rmdir(legacy).catch((error) => { if (!['ENOTEMPTY', 'ENOENT'].includes(error.code)) throw error; });
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    await rm(lease, { recursive: true });
    return true;
  } catch (error) {
    // Unknown or incomplete receipts are occupied, never evidence of abandonment.
    if (!['ENOENT', 'ENOTDIR'].includes(error.code) && !(error instanceof SyntaxError)) console.error(`Build cache was preserved: ${error.message}`);
  } finally { await rm(lock, { recursive: true }); }
}

async function overlapsLease(target) {
  let ancestor = path.dirname(target);
  for (;;) {
    try { await lstat(path.join(ancestor, '.faber-lease')); return true; } catch (error) {
      if (error.code !== 'ENOENT') return true;
    }
    const parent = path.dirname(ancestor);
    if (parent === ancestor) break;
    ancestor = parent;
  }
  const pending = [target];
  while (pending.length) {
    const folder = pending.pop();
    try {
      const meta = await lstat(folder);
      if (meta.isSymbolicLink()) {
        if (folder === target) return true;
        continue;
      }
      if (!meta.isDirectory()) return true;
      for (const entry of await readdir(folder, { withFileTypes: true })) {
        if (entry.name === '.faber-lease') {
          if (folder !== target) return true;
        } else if (entry.isDirectory() && !entry.isSymbolicLink()) pending.push(path.join(folder, entry.name));
      }
    } catch { return true; }
  }
  return false;
}

// The workspace's own package names, read from the copy's Cargo.toml files. What this plain reading cannot follow (a
// glob, a member without a name) throws, so a cache is never marked as adopted with a crate left out.
async function workspaceCrates(copy) {
  const members = (await readFile(path.join(copy, 'Cargo.toml'), 'utf8')).match(/^\s*members\s*=\s*\[([^\]]*)\]/m)?.[1] ?? '';
  const names = [];
  for (const [, member] of members.matchAll(/["']([^"']+)["']/g)) {
    if (/[*?[]/.test(member)) throw new Error(`Cannot read the workspace member pattern ${member}`);
    const manifest = await readFile(path.join(copy, member, 'Cargo.toml'), 'utf8');
    const section = manifest.indexOf('[package]');
    const name = section < 0 ? null : manifest.slice(section).match(/^\s*name\s*=\s*["']([^"']+)["']/m)?.[1];
    if (!name) throw new Error(`Cannot read the package name of ${member}`);
    names.push(name);
  }
  if (names.length === 0) throw new Error(`Cannot read the workspace members of ${copy}`);
  return names;
}

// Every `.fingerprint` folder of a cache: a profile's (`debug`), a target triple's, or the test build's (`faber-test`).
async function fingerprintFolders(folder, depth = 0, found = []) {
  let entries;
  try { entries = await readdir(folder, { withFileTypes: true }); } catch (error) { if (error.code === 'ENOENT') return found; throw error; }
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    if (entry.name === '.fingerprint') found.push(path.join(folder, entry.name));
    else if (depth < 3 && !['deps', 'build', 'incremental', 'examples', '.faber-lease', '.faber-tmp'].includes(entry.name)) {
      await fingerprintFolders(path.join(folder, entry.name), depth + 1, found);
    }
  }
  return found;
}

// A cache moves between copies, and Cargo judges a workspace crate by the file times of its sources, named relative to
// the copy: another copy's newer build would look fresh here and run its code. So a cache remembers the copy that
// built in it last, and a different copy forgets the workspace crates' fingerprints first, which rebuilds only them.
export async function adoptTarget(target, copy) {
  const stamp = path.join(target, '.faber-copy');
  const existing = await lstat(stamp).catch((error) => { if (error.code === 'ENOENT') return null; throw error; });
  if (existing && !existing.isFile()) throw new Error(`Refusing a linked or unusual cache stamp: ${stamp}`);
  if (existing && await readFile(stamp, 'utf8') === copy) {
    // The stamp's time is the cache's last use, which orders the caches a copy may take (`slotOrder`).
    const now = new Date();
    await utimes(stamp, now, now);
    return false;
  }
  const names = await workspaceCrates(copy);
  const own = (entry) => names.some((name) => entry.startsWith(`${name}-`) && /^[0-9a-f]{16}$/.test(entry.slice(name.length + 1)));
  for (const folder of await fingerprintFolders(target)) {
    for (const entry of await readdir(folder, { withFileTypes: true })) {
      if (entry.isDirectory() && own(entry.name)) await rm(path.join(folder, entry.name), { recursive: true, force: true });
    }
  }
  await writeFile(stamp, copy);
  return true;
}

// A cache whose workspace crates were last built from one of the preferred copies (this one, or the lane a landing
// verifies for) has the least to compile again; a cache stamped by a copy that is gone is the next best; the rest go
// least recently adopted first. Before, every lane took the first free cache, so the three bounced between copies.
export function slotOrder(slots, prefer = []) {
  const same = (a, b) => typeof a === 'string' && typeof b === 'string' && (process.platform === 'win32' ? a.toLowerCase() === b.toLowerCase() : a === b);
  const rank = (slot) => {
    const at = prefer.findIndex((copy) => same(slot.copy, copy));
    return at >= 0 ? at : prefer.length + (slot.gone ? 0 : 1);
  };
  return [...slots].sort((a, b) => rank(a) - rank(b) || a.at - b.at || a.index - b.index);
}

async function bySlotAffinity(targets, prefer) {
  if (targets.length < 2 || prefer.length === 0) return targets;
  const slots = await Promise.all(targets.map(async (target, index) => {
    const stamp = path.join(target, '.faber-copy');
    try {
      const stat = await lstat(stamp);
      const copy = stat.isFile() ? await readFile(stamp, 'utf8') : null;
      const gone = copy === null || await lstat(copy).then(() => false, () => true);
      return { target, index, copy, at: stat.mtimeMs, gone };
    } catch { return { target, index, copy: null, at: 0, gone: true }; }
  }));
  return slotOrder(slots, prefer).map((slot) => slot.target);
}

export async function acquireTarget(root, env, signal, prefer = []) {
  let ancestor = systemTemp;
  for (;;) {
    try { await lstat(path.join(ancestor, '.git')); throw new Error(`Temporary directory is inside a Git working tree: ${systemTemp}`); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    const parent = path.dirname(ancestor);
    if (parent === ancestor) break;
    ancestor = parent;
  }
  await directory(tempNamespace);
  let targets = env.CARGO_TARGET_DIR
    ? [path.resolve(root, env.CARGO_TARGET_DIR)]
    : [1, 2, 3].map((slot) => path.join(root, 'target', `build-${slot}`));
  for (const target of targets) await directory(target);
  targets = await bySlotAffinity(await Promise.all(targets.map((target) => realpath(target))), prefer);
  for (;;) {
    signal?.throwIfAborted();
    for (const target of targets) {
      const lease = path.join(target, '.faber-lease');
      try { await mkdir(lease); } catch (error) {
        if (error.code !== 'EEXIST') throw error;
        if (!await reap(target)) continue;
        try { await mkdir(lease); } catch (error) { if (error.code === 'EEXIST') continue; throw error; }
      }
      if (await overlapsLease(target)) {
        await rmdir(lease);
        continue;
      }
      const owner = { pid: process.pid, childPid: null, createdAt: Date.now(), tempName: `run-${randomUUID()}` };
      owner.tempPath = path.join(tempParent(target), owner.tempName);
      const receipt = path.join(lease, 'owner.json');
      const save = async () => {
        await writeFile(path.join(lease, 'owner.new'), JSON.stringify(owner));
        const until = Date.now() + 1000;
        for (;;) {
          try { await rename(path.join(lease, 'owner.new'), receipt); break; } catch (error) {
            // Windows readers and scanners can briefly deny atomic replacement.
            if (process.platform !== 'win32' || !['EPERM', 'EACCES', 'EBUSY'].includes(error.code) || Date.now() >= until) throw error;
            await setTimeout(25);
          }
        }
      };
      const temp = owner.tempPath;
      try {
        await save();
        await directory(temp);
      } catch (error) {
        await rm(lease, { recursive: true });
        throw error;
      }
      return {
        target,
        // No CARGO_INCREMENTAL unless the caller set one: Cargo's dev profile then compiles only the workspace's own crates
        // incrementally (a one-line change no longer recompiles a whole crate), which sccache passes through, while sccache
        // refuses every compile when the variable says 1. The cost is disk: a cache grew from about 5 to 13 GB on
        // 2026-11-11. `FABER_INCREMENTAL=0` turns it off.
        env: { ...env, CARGO_TARGET_DIR: target, ...(env.FABER_INCREMENTAL === '0' && env.CARGO_INCREMENTAL === undefined ? { CARGO_INCREMENTAL: '0' } : {}), TEMP: temp, TMP: temp, TMPDIR: temp },
        async child(pid, contained) { owner.childPid = pid; owner.contained = contained; await save(); },
        async interrupt() { owner.childPid = null; delete owner.contained; await save(); },
        async release() {
          // Keep the receipt when cleanup fails, so a later invocation can retry safely.
          await removeTemp(tempParent(target), owner.tempName);
          await rm(lease, { recursive: true });
        },
      };
    }
    await setTimeout(250, undefined, { signal });
  }
}

async function stopTree(child, signal) {
  if (process.platform === 'win32') {
    const taskkill = path.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'taskkill.exe');
    await execute(taskkill, ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, shell: false });
    return;
  }
  try { process.kill(-child.pid, signal); } catch (error) { if (error.code === 'ESRCH') return; throw error; }
  const until = Date.now() + 3000;
  while (Date.now() < until) {
    try { process.kill(-child.pid, 0); } catch (error) { if (error.code === 'ESRCH') return; throw error; }
    await setTimeout(25);
  }
  throw new Error('The build process group is still present');
}

function forwardSignals(controller, getChild, getLease, getReceipt) {
  let interrupted = false;
  let stopping = false;
  let exitCode;
  let forwarding = Promise.resolve();
  const signals = ['SIGINT', 'SIGTERM', 'SIGHUP'];
  const handlers = signals.map((signal) => () => {
    const child = getChild();
    if (!child) { controller.abort(); return; }
    if (stopping) return;
    stopping = true;
    interrupted = true;
    exitCode = signal === 'SIGINT' ? 130 : 143;
    forwarding = forwarding.then(async () => {
      await getReceipt().catch(() => {});
      await getLease().interrupt();
      try {
        await stopTree(child, signal);
        await getLease().child(child.pid, child.contained);
        interrupted = false;
      } catch (error) {
        console.error(`Interrupted build: temporary files and cache reservation were preserved because the process tree could not be confirmed stopped: ${error.message}`);
        child.kill(signal);
      }
    }).catch((error) => {
      console.error(`Could not preserve the interrupted build receipt; the child was left running: ${error.message}`);
    });
  });
  signals.forEach((signal, i) => process.on(signal, handlers[i]));
  return {
    get interrupted() { return interrupted; },
    get exitCode() { return exitCode; },
    async stop() {
      for (;;) {
        const pending = forwarding;
        await pending;
        if (pending === forwarding) break;
      }
      signals.forEach((signal, i) => process.off(signal, handlers[i]));
    },
  };
}

// The results ledger: a proof that passed on a tree is not run again on the same tree. A command is a proof when its
// only product is what it prints and its exit code (check, clippy, test without --no-run, fmt --check); its key is the
// tree's Git hash with uncommitted and untracked files, the arguments, the toolchain and the variables that change
// what the compiler or the tests do. Only passes are kept: a failure is run again, since the next try may differ.
const LEDGER_OUTPUT = 16 << 20;
const LEDGER_KEEP = { entries: 300, age: 3 * DAY };

export function proofCommand(args) {
  const split = args.indexOf('--');
  const own = split < 0 ? args : args.slice(0, split);
  const sub = own.find((arg) => !arg.startsWith('-'));
  if (!sub || sub !== own[0]) return null;
  if (sub === 'check' || sub === 'clippy') return sub;
  if (sub === 'test' && !own.includes('--no-run')) return sub;
  if (sub === 'fmt' && args.includes('--check')) return sub;
  return null;
}

// What passing `check` proves, Clippy with warnings denied proves too: a lane's Clippy covers a later check of the tree.
export function ledgerQueries(args) {
  const queries = [args];
  if (proofCommand(args) === 'check') {
    const split = args.indexOf('--');
    const own = split < 0 ? args : args.slice(0, split);
    const after = split < 0 ? [] : args.slice(split + 1);
    if (!after.length) queries.push(['clippy', ...own.slice(1), '--', '-D', 'warnings']);
  }
  return queries;
}

// Variables that change the result of a proof. The launcher's own (budget, priority, statistics, slot choice) do not.
// Every Cargo setting counts except where its files go and its terminal; RUSTC* picks the compiler. Files Git ignores and
// other variables a test might read are not part of the key: a proof that depends on them runs with FABER_FRESH=1.
const RESULT_VARIABLES = /^((?!CARGO_(HOME|TARGET_DIR|MAKEFLAGS|INCREMENTAL|TERM_.*)$)CARGO_.*|RUST(C|DOC)?(_.*)?|RUSTFLAGS|RUSTDOCFLAGS|RUSTUP_TOOLCHAIN|CI|FABER_(?!JOBSERVER$|JOB_STATS$|BUILD_PRIORITY$|BUILD_BUDGET$|CARGO_AFFINITY$|FRESH$|INCREMENTAL$|COPY_LINK$).*)$/;

export function ledgerKey({ tree, toolchain, args, env, dir = '' }) {
  const variables = Object.keys(env).filter((name) => RESULT_VARIABLES.test(name)).sort().map((name) => [name, env[name]]);
  return createHash('sha256').update(JSON.stringify({ v: 2, tree, toolchain, dir, args, variables })).digest('hex').slice(0, 40);
}

// The Git hash of the copy as it is now, uncommitted and untracked files included (ignored ones are not part of it).
export async function treeHash(copy, run = execute) {
  const git = (args, env) => run('git', args, { cwd: copy, windowsHide: true, maxBuffer: 64 << 20, env: env ?? process.env }).then((r) => r.stdout.trim());
  if (!await git(['status', '--porcelain', '--untracked-files=all', '--ignore-submodules=none'])) return git(['rev-parse', 'HEAD^{tree}']);
  // A private index, seeded from the copy's own so unchanged files are not hashed again; the copy's index is untouched.
  const index = path.join(tempNamespace, `index-${randomUUID()}`);
  try {
    const own = await git(['rev-parse', '--path-format=absolute', '--git-path', 'index']);
    await writeFile(index, await readFile(own).catch(() => Buffer.alloc(0)));
    const env = { ...process.env, GIT_INDEX_FILE: index };
    await git(['add', '--all', '--', '.'], env);
    return await git(['write-tree'], env);
  } finally { await rm(index, { force: true }); }
}

function ledgerFolder(root) { return path.join(root, 'target', '.faber-ledger'); }

export async function ledgerFind(root, keys, now = Date.now()) {
  for (const key of keys) {
    const file = path.join(ledgerFolder(root), `${key}.json`);
    try {
      const entry = JSON.parse(await readFile(file, 'utf8'));
      if (entry.code !== 0 || !(now - entry.at < LEDGER_KEEP.age)) continue;
      const out = await readFile(path.join(ledgerFolder(root), `${key}.out`)).catch(() => Buffer.alloc(0));
      const err = await readFile(path.join(ledgerFolder(root), `${key}.err`)).catch(() => Buffer.alloc(0));
      return { ...entry, key, stdout: out, stderr: err };
    } catch (error) { if (error.code !== 'ENOENT' && !(error instanceof SyntaxError)) throw error; }
  }
  return null;
}

export async function ledgerRecord(root, key, entry, capture, now = Date.now()) {
  if (!capture || capture.bytes > LEDGER_OUTPUT) return false;
  const folder = ledgerFolder(root);
  await directory(folder);
  const temp = (name) => path.join(folder, `${name}.${process.pid}.new`);
  await writeFile(temp(`${key}.out`), Buffer.concat(capture.stdout));
  await writeFile(temp(`${key}.err`), Buffer.concat(capture.stderr));
  await rename(temp(`${key}.out`), path.join(folder, `${key}.out`));
  await rename(temp(`${key}.err`), path.join(folder, `${key}.err`));
  // The entry goes last: a reader that finds it finds its output too.
  await writeFile(temp(`${key}.json`), JSON.stringify({ ...entry, at: now }));
  await rename(temp(`${key}.json`), path.join(folder, `${key}.json`));
  await ledgerPrune(folder, now);
  return true;
}

async function ledgerPrune(folder, now) {
  const entries = [];
  for (const name of await readdir(folder)) {
    if (!name.endsWith('.json')) continue;
    const stat = await lstat(path.join(folder, name)).catch(() => null);
    if (stat) entries.push({ key: name.slice(0, -5), at: stat.mtimeMs });
  }
  entries.sort((a, b) => b.at - a.at);
  for (const [i, entry] of entries.entries()) {
    if (i < LEDGER_KEEP.entries && now - entry.at < LEDGER_KEEP.age) continue;
    for (const ext of ['.json', '.out', '.err']) await rm(path.join(folder, entry.key + ext), { force: true });
  }
}

// A lock directory's owner died, or it never got one within `grace` milliseconds.
async function abandoned(lock, grace) {
  const owner = Number(await readFile(path.join(lock, 'owner'), 'utf8').catch(() => ''));
  if (owner) return dead(owner);
  return lstat(lock).then((s) => Date.now() - s.mtimeMs > grace, () => false);
}

// Moves an abandoned lock aside before deleting it: a rename succeeds for one waiter only, so two waiters that saw the
// same dead owner cannot both take over, and the second never deletes the lock the first one just made. When the
// moved lock turns out to be a live one (it was replaced between the check and the rename), it goes back.
async function takeOver(lock) {
  const aside = `${lock}.stale-${randomUUID()}`;
  try { await rename(lock, aside); } catch (error) { if (['ENOENT', 'EPERM', 'EACCES', 'EBUSY', 'ENOTEMPTY', 'EEXIST'].includes(error.code)) return; throw error; }
  if (!await abandoned(aside, 0)) {
    try { await rename(aside, lock); return; } catch { /* someone holds the lock again; the moved one is released */ }
  }
  await rm(aside, { recursive: true, force: true });
}

// One lane runs a proof at a time per key; a lane asking for the same proof on the same tree waits for its result
// instead of compiling the same thing next to it. A lock whose owner died is taken over.
export async function ledgerLock(root, key, signal, said = () => {}) {
  const folder = ledgerFolder(root);
  await directory(folder);
  const lock = path.join(folder, `${key}.lock`);
  let waited = false;
  for (;;) {
    signal?.throwIfAborted();
    try {
      await mkdir(lock);
      await writeFile(path.join(lock, 'owner'), String(process.pid));
      return { waited, release: () => rm(lock, { recursive: true, force: true }) };
    } catch (error) {
      if (error.code !== 'EEXIST') throw error;
      // A lock without an owner yet is a moment old; one whose owner is gone is free.
      if (await abandoned(lock, 60000)) { await takeOver(lock); continue; }
      if (!waited) said();
      waited = true;
      await setTimeout(1000, undefined, { signal });
    }
  }
}

// The PC's one build budget (Windows): a named semaphore all lanes' Cargo and rustc take tokens from, instead of each
// lane's own `jobs`. Each Cargo also keeps its one implicit token, so a lane alone keeps its speed and three lanes
// together stop at about half the processor. `FABER_BUILD_BUDGET` sets the shared tokens; CI keeps Cargo's own count.
export const BUDGET_NAME = 'faber-build-jobs';
export function budgetTokens(logical, configured) {
  const set = Number.parseInt(configured ?? '', 10);
  if (Number.isSafeInteger(set) && set > 0) return Math.min(set, 256);
  return Math.max(1, Math.floor(logical / 2) - 2);
}

export function budgetEnv(env, logical = os.cpus().length) {
  if (process.platform !== 'win32' || env.CI === 'true' || env.CARGO_MAKEFLAGS || env.MAKEFLAGS) return env;
  return {
    ...env,
    FABER_JOBSERVER: `${BUDGET_NAME}:${budgetTokens(logical, env.FABER_BUILD_BUDGET)}`,
    CARGO_MAKEFLAGS: `-j --jobserver-auth=${BUDGET_NAME}`,
    FABER_BUILD_PRIORITY: env.FABER_BUILD_PRIORITY ?? 'below',
  };
}

// The compiler cache server (sccache) can die mid-build; the build then fails before any test ran.
export const CACHE_SERVER_DIED = /sccache[^\n]*(?:10054|connection|server)/i;

// Every copy builds in a cache through one path, a link named after the cache that points at the copy leasing it. The
// workspace crates then keep one identity per cache whichever copy builds: Cargo's units, the compiler's incremental
// state and the compile cache's keys stay valid when another copy takes the cache, so only what differs is compiled
// again. Before, each copy's own path made them other crates, and a landing's fresh copy compiled the workspace whole.
export function copyLinkPath(root, target) {
  const id = createHash('sha256').update(process.platform === 'win32' ? target.toLowerCase() : target).digest('hex').slice(0, 12);
  return path.join(root, 'target', '.faber-copies', `${path.basename(target)}-${id}`);
}

export async function linkCopy(root, target, copy, cwd) {
  const inside = path.relative(copy, cwd);
  if (inside.startsWith('..') || path.isAbsolute(inside)) return cwd;
  const link = copyLinkPath(root, target);
  await directory(path.dirname(link));
  let current = null;
  try {
    const stat = await lstat(link);
    if (!stat.isSymbolicLink()) throw new Error(`Refusing a build copy path that is not a link: ${link}`);
    current = path.resolve(await readlink(link));
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  const same = current !== null && (process.platform === 'win32' ? current.toLowerCase() === copy.toLowerCase() : current === copy);
  if (!same) {
    // Only the link goes: unlink never follows it into the copy it pointed at.
    if (current !== null) await unlink(link);
    await symlink(copy, link, process.platform === 'win32' ? 'junction' : 'dir');
  }
  return path.join(link, inside);
}

export async function runChild(root, command, args, env, cwd = root, copy = null, jobRunner = null, options = {}) {
  const controller = new AbortController();
  let child;
  let lease;
  let receiptSaved = Promise.resolve();
  const signals = forwardSignals(controller, () => child, () => lease, () => receiptSaved);
  let finished = false;
  const capture = options.capture;
  // Stderr is read to spot a dead cache server and passed through unchanged; stdout and stdin stay the caller's, unless
  // the result is recorded for other lanes (`capture`), when stdout is passed through and kept as well.
  const attempt = async (childEnv) => {
    finished = false;
    if (capture) Object.assign(capture, { stdout: [], stderr: [], bytes: 0 });
    const keep = (stream, chunk) => {
      if (!capture || capture.bytes > LEDGER_OUTPUT) return;
      capture.bytes += chunk.length;
      capture[stream].push(chunk);
    };
    child = spawn(jobRunner ?? command, jobRunner ? [command, ...args] : args, { cwd, env: childEnv, stdio: ['inherit', capture ? 'pipe' : 'inherit', 'pipe'], shell: false, windowsHide: true, detached: process.platform !== 'win32' });
    child.contained = jobRunner ? 'job' : process.platform === 'win32' ? undefined : 'group';
    let tail = '';
    child.stdout?.on('data', (chunk) => { process.stdout.write(chunk); keep('stdout', chunk); });
    child.stderr.on('data', (chunk) => { process.stderr.write(chunk); keep('stderr', chunk); tail = (tail + chunk).slice(-16384); });
    const result = new Promise((resolve, reject) => {
      child.once('error', (error) => { finished = true; reject(error); });
      // A background server the build started can keep the pipe open, so the last output is only given a moment.
      child.once('exit', (code, signal) => {
        finished = true;
        Promise.race([new Promise((done) => child.stderr.once('end', done)), setTimeout(300)]).then(() => resolve({ code: code ?? (signal === 'SIGINT' ? 130 : 143), tail }));
      });
    });
    let receiptError;
    if (child.pid) {
      receiptSaved = lease.child(child.pid, child.contained);
      try { await receiptSaved; } catch (error) { receiptError = error; }
    }
    const outcome = await result;
    if (receiptError) throw receiptError;
    return outcome;
  };
  try {
    lease = await acquireTarget(root, env, controller.signal, options.prefer ?? (copy ? [copy] : []));
    if (copy) await adoptTarget(lease.target, copy);
    if (copy && options.link) cwd = await linkCopy(root, lease.target, copy, cwd);
    controller.signal.throwIfAborted();
    let { code, tail } = await attempt(lease.env);
    if (code !== 0 && !signals.interrupted && lease.env.RUSTC_WRAPPER !== '' && CACHE_SERVER_DIED.test(tail)) {
      console.error('The compiler cache server died; running this build once more without it.');
      ({ code } = await attempt({ ...lease.env, RUSTC_WRAPPER: '' }));
    }
    return signals.exitCode ?? code;
  } finally {
    await signals.stop();
    if (lease && !signals.interrupted && (!child || finished)) {
      try { await lease.release(); } catch (error) { console.error(`Temporary build files could not be removed: ${error.message}`); }
    }
  }
}

export function validateCargoArgs(args, tauri = false) {
  for (const arg of args) {
    if (arg === '--' && !tauri) break;
    if (/^--(?:target|build)-dir(?:=|$)/.test(arg) || /build\.(?:target-dir|build-dir)\s*=/.test(arg)) {
      throw new Error('Set CARGO_TARGET_DIR instead of output-directory arguments so the build cache can be leased safely');
    }
  }
}

const sourceRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

// A small Rust helper compiled once per source digest into the shared tools cache, and reused by every checkout.
async function compiledTool(root, name, source, extraSources = []) {
  const tools = path.join(root, 'target', '.faber-tools');
  await directory(tools);
  const hash = createHash('sha256').update(process.platform).update(process.arch);
  for (const file of [source, ...extraSources]) hash.update(await readFile(file));
  const program = path.join(tools, `${name}-${hash.digest('hex').slice(0, 16)}${process.platform === 'win32' ? '.exe' : ''}`);
  let existing;
  try { existing = await lstat(program); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (existing && (!existing.isFile() || existing.isSymbolicLink())) throw new Error(`Refusing linked build maintenance helper: ${program}`);
  if (!existing) {
    const controller = new AbortController();
    let lease;
    let compiler;
    let finished = false;
    let receiptSaved = Promise.resolve();
    const signals = forwardSignals(controller, () => compiler, () => lease, () => receiptSaved);
    try {
      lease = await acquireTarget(root, { ...process.env, CARGO_TARGET_DIR: tools }, controller.signal);
      controller.signal.throwIfAborted();
      const compiled = path.join(lease.target, '.faber-lease', path.basename(program));
      compiler = spawn('rustc', ['--edition=2024', '-O', source, '-o', compiled], {
        cwd: sourceRoot, windowsHide: true, env: lease.env, shell: false, stdio: 'inherit', detached: process.platform !== 'win32',
      });
      const settled = new Promise((resolve) => {
        compiler.once('error', (error) => { finished = true; resolve({ error }); });
        compiler.once('close', (code, signal) => {
          finished = true;
          resolve(code === 0 ? {} : { error: new Error(`rustc exited with ${code ?? signal}`) });
        });
      });
      let receiptError;
      if (compiler.pid) {
        receiptSaved = lease.child(compiler.pid);
        try { await receiptSaved; } catch (error) { receiptError = error; }
      }
      const result = await settled;
      if (result.error) throw result.error;
      if (receiptError) throw receiptError;
      try { await rename(compiled, program); } catch (error) {
        // Another checkout may have just published this exact source digest.
        const winner = await lstat(program).catch(() => null);
        if (!['EEXIST', 'EPERM', 'EACCES'].includes(error.code) || !winner?.isFile() || winner.isSymbolicLink()) throw error;
      }
    } catch (error) {
      throw new Error(`Build helper ${name} could not be compiled; rustc must be installed and available: ${error.message}`);
    } finally {
      await signals.stop();
      if (lease && !signals.interrupted && (!compiler || finished)) {
        try { await lease.release(); } catch (error) { console.error(`Temporary maintenance files could not be removed: ${error.message}`); }
      }
    }
  }
  return program;
}

export async function maintainStorage(root, ownTarget) {
  const program = await compiledTool(root, 'storage-sweep', path.join(sourceRoot, 'scripts', 'storage-sweep.rs'), [path.join(sourceRoot, 'testkit', 'src', 'sweep.rs')]);
  const result = await execute(program, [root, ownTarget], { cwd: sourceRoot, windowsHide: true });
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  return program;
}

async function main() {
  const cwd = process.cwd();
  const common = execFileSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'], { cwd, encoding: 'utf8', windowsHide: true }).trim();
  if (!path.isAbsolute(common) || path.basename(common) !== '.git') throw new Error('Cannot locate the main checkout safely');
  const root = path.dirname(await realpath(common));
  const copy = await realpath(execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd, encoding: 'utf8', windowsHide: true }).trim());
  if (!(await lstat(path.join(root, '.git'))).isDirectory()) throw new Error('Main checkout metadata is not a directory');
  const args = process.argv.slice(2);
  const tauri = args[0] === '--tauri';
  if (tauri) args.shift();
  if (args.length === 0) throw new Error('Usage: node scripts/cargo.mjs [--tauri] <arguments>');
  validateCargoArgs(args, tauri);
  let env = { ...process.env };
  if (env.CARGO_TARGET_DIR) env.CARGO_TARGET_DIR = path.resolve(cwd, env.CARGO_TARGET_DIR);
  // The landing's fresh copy prefers the cache of the lane it lands for, whose compiler state is closest to its tree.
  const prefer = [copy, ...(env.FABER_CARGO_AFFINITY ? [await realpath(env.FABER_CARGO_AFFINITY).catch(() => env.FABER_CARGO_AFFINITY)] : [])];
  const proof = tauri ? null : proofCommand(args);
  let key = null;
  if (proof && env.FABER_FRESH !== '1') {
    // The folder counts: `cargo test` in a package folder proves that package only.
    const dir = path.relative(copy, await realpath(cwd)).split(path.sep).join('/');
    const [tree, toolchain] = await Promise.all([treeHash(copy), execute(env.RUSTC || 'rustc', ['-vV'], { cwd, windowsHide: true }).then((r) => r.stdout.trim())]);
    const keys = ledgerQueries(args).map((query) => ledgerKey({ tree, toolchain, args: query, env, dir }));
    key = keys[0];
    const said = () => console.error('Waiting: another lane runs this proof on the same files; its result serves both.');
    const lock = await ledgerLock(root, key, undefined, said);
    try {
      const found = await ledgerFind(root, keys);
      if (found) {
        process.stdout.write(found.stdout);
        process.stderr.write(found.stderr);
        const when = new Date(found.at).toISOString().slice(0, 16).replace('T', ' ');
        console.error(`Passed already on these exact files (${when} UTC, \`${found.args.join(' ')}\`); nothing compiled. FABER_FRESH=1 runs it again.`);
        process.exitCode = 0;
        return;
      }
      process.exitCode = await build(root, args, tauri, env, cwd, copy, prefer, key, tree);
    } finally { await lock.release(); }
    return;
  }
  process.exitCode = await build(root, args, tauri, env, cwd, copy, prefer, null, null);
}

async function build(root, args, tauri, env, cwd, copy, prefer, key, tree) {
  await maintainStorage(root, env.CARGO_TARGET_DIR ?? path.join(root, 'target'));
  // Windows has no process groups, so the build runs inside a kill-on-close job: a dead launcher then proves no compiler survives.
  const jobRunner = process.platform === 'win32' ? await compiledTool(root, 'job-run', path.join(sourceRoot, 'scripts', 'job-run.rs')) : null;
  if (jobRunner) env = budgetEnv(env);
  const capture = key ? {} : undefined;
  const command = tauri ? process.execPath : 'cargo';
  const commandArgs = tauri ? [path.join(cwd, 'node_modules', '@tauri-apps', 'cli', 'tauri.js'), ...args] : args;
  const code = await runChild(root, command, commandArgs, env, cwd, copy, jobRunner, { prefer, link: !tauri && env.FABER_COPY_LINK !== '0', capture });
  // Files edited while the proof ran make it a proof of neither tree, so nothing is recorded.
  if (key && code === 0 && await treeHash(copy).then((now) => now === tree, () => false)) {
    try { await ledgerRecord(root, key, { code, args, tree }, capture); } catch (error) { console.error(`The passing result could not be recorded: ${error.message}`); }
  }
  return code;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
