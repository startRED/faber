// Share a bounded cache pool across checkouts, while keeping each invocation's temp private.
import { spawn, execFile, execFileSync } from 'node:child_process';
import { mkdir, lstat, readFile, writeFile, rename, rm, rmdir, realpath, readdir } from 'node:fs/promises';
import path from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { setTimeout } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const DAY = 86400000;
const execute = promisify(execFile);

async function directory(folder) {
  const parent = path.dirname(folder);
  if (parent !== folder) await directory(parent);
  let stat;
  try { stat = await lstat(folder); } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    try { await mkdir(folder); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    stat = await lstat(folder);
  }
  if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error(`Expected a real directory, not a link: ${folder}`);
}

function dead(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try { process.kill(pid, 0); return false; } catch (error) { return error.code === 'ESRCH'; }
}

async function removeTemp(target, name) {
  if (!/^run-[a-zA-Z0-9-]+$/.test(name)) throw new Error('Invalid owned temporary directory');
  const parent = path.join(target, '.faber-tmp');
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
  try { await mkdir(lock); } catch (error) {
    // An inaccessible reaper lock never authorizes touching an occupied cache.
    if (['EEXIST', 'EPERM', 'EACCES', 'EBUSY'].includes(error.code)) return false;
    throw error;
  }
  try {
    const stat = await lstat(lease);
    if (!stat.isDirectory() || stat.isSymbolicLink() || Date.now() - stat.mtimeMs < DAY) return;
    const receipt = path.join(lease, 'owner.json');
    if (!(await lstat(receipt)).isFile() || (await lstat(receipt)).isSymbolicLink()) return;
    const owner = JSON.parse(await readFile(receipt, 'utf8'));
    if (!Number.isFinite(owner.createdAt) || Date.now() - owner.createdAt < DAY || !dead(owner.pid) || !dead(owner.childPid)) return;
    if (await overlapsLease(target)) return;
    await removeTemp(target, owner.tempName);
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
  if (existing && await readFile(stamp, 'utf8') === copy) return false;
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

export async function acquireTarget(root, env, signal) {
  const targets = env.CARGO_TARGET_DIR
    ? [path.resolve(root, env.CARGO_TARGET_DIR)]
    : [1, 2, 3].map((slot) => path.join(root, 'target', `build-${slot}`));
  for (const target of targets) await directory(target);
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
      const temp = path.join(target, '.faber-tmp', owner.tempName);
      try {
        await save();
        await directory(temp);
      } catch (error) {
        await rm(lease, { recursive: true });
        throw error;
      }
      return {
        target,
        env: { ...env, CARGO_TARGET_DIR: target, CARGO_INCREMENTAL: env.CARGO_INCREMENTAL ?? '0', TEMP: temp, TMP: temp, TMPDIR: temp },
        async child(pid) { owner.childPid = pid; await save(); },
        async interrupt() { owner.childPid = null; await save(); },
        async release() {
          // Keep the receipt when cleanup fails, so a later invocation can retry safely.
          await removeTemp(target, owner.tempName);
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
        await getLease().child(child.pid);
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

export async function runChild(root, command, args, env, cwd = root, copy = null) {
  const controller = new AbortController();
  let child;
  let lease;
  let receiptSaved = Promise.resolve();
  const signals = forwardSignals(controller, () => child, () => lease, () => receiptSaved);
  let finished = false;
  try {
    lease = await acquireTarget(root, env, controller.signal);
    if (copy) await adoptTarget(lease.target, copy);
    controller.signal.throwIfAborted();
    child = spawn(command, args, { cwd, env: lease.env, stdio: 'inherit', shell: false, windowsHide: true, detached: process.platform !== 'win32' });
    const result = new Promise((resolve, reject) => {
      child.once('error', (error) => { finished = true; reject(error); });
      child.once('exit', (code, signal) => { finished = true; resolve(code ?? (signal === 'SIGINT' ? 130 : 143)); });
    });
    let receiptError;
    if (child.pid) {
      receiptSaved = lease.child(child.pid);
      try { await receiptSaved; } catch (error) { receiptError = error; }
    }
    const code = await result;
    if (receiptError) throw receiptError;
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

export async function maintainStorage(root, ownTarget) {
  const tools = path.join(root, 'target', '.faber-tools');
  await directory(tools);
  const sourceRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
  const source = path.join(sourceRoot, 'scripts', 'storage-sweep.rs');
  const hash = createHash('sha256').update(process.platform).update(process.arch);
  for (const file of [source, path.join(sourceRoot, 'testkit', 'src', 'sweep.rs')]) hash.update(await readFile(file));
  const program = path.join(tools, `storage-sweep-${hash.digest('hex').slice(0, 16)}${process.platform === 'win32' ? '.exe' : ''}`);
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
      const compiled = path.join(lease.env.TEMP, path.basename(program));
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
      throw new Error(`Build storage maintenance could not be compiled; rustc must be installed and available: ${error.message}`);
    } finally {
      await signals.stop();
      if (lease && !signals.interrupted && (!compiler || finished)) {
        try { await lease.release(); } catch (error) { console.error(`Temporary maintenance files could not be removed: ${error.message}`); }
      }
    }
  }
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
  const env = { ...process.env };
  if (env.CARGO_TARGET_DIR) env.CARGO_TARGET_DIR = path.resolve(cwd, env.CARGO_TARGET_DIR);
  await maintainStorage(root, env.CARGO_TARGET_DIR ?? path.join(root, 'target'));
  process.exitCode = await runChild(root, tauri ? process.execPath : 'cargo', tauri ? [path.join(cwd, 'node_modules', '@tauri-apps', 'cli', 'tauri.js'), ...args] : args, env, cwd, copy);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
