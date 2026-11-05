import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import fsPromises from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { existsSync, readFileSync, writeFileSync, realpathSync } from 'node:fs';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm, symlink, utimes, realpath } from 'node:fs/promises';
import { syncBuiltinESMExports } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import * as launcher from './cargo.mjs';

function tempParent(target) {
  const canonical = path.resolve(target);
  const namespace = process.platform === 'win32' ? 'faber-cargo' : `faber-cargo-${process.getuid()}`;
  return path.join(realpathSync.native(os.tmpdir()), namespace, createHash('sha256').update(process.platform === 'win32' ? canonical.toLowerCase() : canonical).digest('hex').slice(0, 32));
}

test('temporary namespaces separate POSIX users and preserve the Windows name', () => {
  assert.equal(launcher.tempNamespaceName('linux', 1000), 'faber-cargo-1000');
  assert.equal(launcher.tempNamespaceName('linux', 1001), 'faber-cargo-1001');
  assert.equal(launcher.tempNamespaceName('darwin', 501), 'faber-cargo-501');
  assert.equal(launcher.tempNamespaceName('win32', undefined), 'faber-cargo');
});

test('POSIX namespace validation rejects links, foreign owners and writable modes with injected stats', () => {
  const safe = { isDirectory: () => true, isSymbolicLink: () => false, uid: 1000, mode: 0o40700 };
  for (const mode of [0o40700, 0o40750, 0o40755]) {
    assert.doesNotThrow(() => launcher.validateTempNamespace({ ...safe, mode }, 1000, '/tmp/faber-cargo-1000'));
  }
  for (const unsafe of [
    { uid: 1001 }, { mode: 0o40720 }, { mode: 0o40702 }, { mode: 0o40777 },
    { isDirectory: () => false }, { isSymbolicLink: () => true },
  ]) {
    assert.throws(() => launcher.validateTempNamespace({ ...safe, ...unsafe }, 1000, '/tmp/faber-cargo-1000'), /Refusing unsafe build temporary namespace.*faber-cargo-1000/);
  }
});

test('POSIX temp ancestors require root or current ownership and sticky shared directories with injected stats', () => {
  const safe = { isDirectory: () => true, isSymbolicLink: () => false, uid: 1000, mode: 0o40755 };
  for (const uid of [0, 1000]) {
    for (const mode of [0o40700, 0o40755, 0o41777, 0o41720, 0o41702]) {
      assert.doesNotThrow(() => launcher.validateTempAncestor({ ...safe, uid, mode }, 1000, '/tmp/shared'));
    }
  }
  for (const unsafe of [
    { uid: 1001 }, { uid: 1001, mode: 0o41777 }, { mode: 0o40720 }, { mode: 0o40702 }, { mode: 0o40777 },
    { isDirectory: () => false }, { isSymbolicLink: () => true },
  ]) {
    assert.throws(() => launcher.validateTempAncestor({ ...safe, ...unsafe }, 1000, '/tmp/shared'), /Refusing unsafe build temporary ancestor.*\/tmp\/shared/);
  }
});

test('POSIX refuses a writable non-sticky temp parent without deleting files or leases', { skip: process.platform === 'win32' }, async (t) => {
  const root = await fixture(t);
  const script = `import assert from 'node:assert/strict'; import { chmod, lstat, readFile, writeFile } from 'node:fs/promises'; import path from 'node:path';
    import { acquireTarget } from ${JSON.stringify(new URL('./cargo.mjs', import.meta.url).href)};
    const root = process.argv[1];
    const lease = await acquireTarget(root, {});
    const keep = path.join(lease.env.TEMP, 'keep'); await writeFile(keep, 'safe');
    try {
      await chmod(root, 0o777);
      await assert.rejects(lease.release(), /Refusing unsafe build temporary ancestor/);
      await assert.rejects(acquireTarget(root, {}), /Refusing unsafe build temporary ancestor/);
      assert.equal(await readFile(keep, 'utf8'), 'safe');
      assert.ok((await lstat(path.join(lease.target, '.faber-lease'))).isDirectory());
      await chmod(root, 0o1777);
      const next = await acquireTarget(root, { CARGO_TARGET_DIR: 'custom' }); await next.release();
    } finally { await chmod(root, 0o700); await lease.release(); }`;
  const result = childProcess.spawnSync(process.execPath, ['--input-type=module', '-e', script, root], {
    env: { ...process.env, TEMP: root, TMP: root, TMPDIR: root }, encoding: 'utf8', windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
});

test('POSIX uses a 0700 per-user namespace beside an unwritable shared namespace', { skip: process.platform === 'win32' }, async (t) => {
  const root = await fixture(t);
  await mkdir(path.join(root, 'faber-cargo'), { mode: 0o555 });
  const script = `import assert from 'node:assert/strict'; import { lstat } from 'node:fs/promises'; import path from 'node:path';
    import { acquireTarget } from ${JSON.stringify(new URL('./cargo.mjs', import.meta.url).href)};
    const lease = await acquireTarget(process.argv[1], {});
    try {
      const namespace = path.dirname(path.dirname(lease.env.TEMP));
      assert.equal(path.basename(namespace), 'faber-cargo-' + process.getuid());
      const stat = await lstat(namespace);
      assert.equal(stat.uid, process.getuid()); assert.equal(stat.mode & 0o777, 0o700);
    } finally { await lease.release(); }`;
  const result = childProcess.spawnSync(process.execPath, ['--input-type=module', '-e', script, root], {
    env: { ...process.env, TEMP: root, TMP: root, TMPDIR: root }, encoding: 'utf8', windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
});

test('POSIX refuses unsafe namespace ancestors before allocation or deletion', { skip: process.platform === 'win32' }, async (t) => {
  const root = await fixture(t);
  const script = `import assert from 'node:assert/strict'; import { chmod, lstat, readFile, writeFile } from 'node:fs/promises'; import path from 'node:path';
    import { acquireTarget } from ${JSON.stringify(new URL('./cargo.mjs', import.meta.url).href)};
    const lease = await acquireTarget(process.argv[1], {});
    const namespace = path.dirname(path.dirname(lease.env.TEMP));
    const keep = path.join(lease.env.TEMP, 'keep'); await writeFile(keep, 'safe');
    await chmod(namespace, 0o777);
    try {
      await assert.rejects(lease.release(), /Refusing unsafe build temporary namespace/);
      assert.equal(await readFile(keep, 'utf8'), 'safe');
      assert.ok((await lstat(path.join(lease.target, '.faber-lease'))).isDirectory());
      await assert.rejects(acquireTarget(process.argv[1], {}), /Refusing unsafe build temporary namespace/);
    } finally { await chmod(namespace, 0o700); await lease.release(); }`;
  const result = childProcess.spawnSync(process.execPath, ['--input-type=module', '-e', script, root], {
    env: { ...process.env, TEMP: root, TMP: root, TMPDIR: root }, encoding: 'utf8', windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
});

async function fixture(t) {
  const root = await mkdtemp(path.join(await realpath(os.tmpdir()), 'faber-storage-test-'));
  await mkdir(path.dirname(tempParent(root)), { recursive: true, mode: 0o700 });
  t.after(async () => {
    for (const target of [1, 2, 3].map((i) => path.join(root, 'target', `build-${i}`)).concat([path.join(root, 'target', '.faber-tools'), path.join(root, 'custom'), path.join(root, 'target', 'build-1', 'nested')])) {
      await rm(tempParent(target), { recursive: true, force: true });
    }
    await rm(root, { recursive: true, force: true });
  });
  return root;
}

test('private temp has no Git repository ancestor', async (t) => {
  const root = await fixture(t);
  childProcess.execFileSync('git', ['init', '-q', root]);
  const lease = await launcher.acquireTarget(root, {});
  try {
    let folder = lease.env.TEMP;
    for (;;) {
      assert.equal(existsSync(path.join(folder, '.git')), false, `Git repository ancestor: ${folder}`);
      const parent = path.dirname(folder);
      if (parent === folder) break;
      folder = parent;
    }
  } finally { await lease.release(); }
});

test('refuses a system temp inside a Git repository', async (t) => {
  const root = await fixture(t);
  childProcess.execFileSync('git', ['init', '-q', root]);
  const script = `import { acquireTarget } from ${JSON.stringify(new URL('./cargo.mjs', import.meta.url).href)}; await acquireTarget(process.argv[1], {});`;
  const result = childProcess.spawnSync(process.execPath, ['--input-type=module', '-e', script, root], {
    env: { ...process.env, TEMP: root, TMP: root, TMPDIR: root }, encoding: 'utf8', windowsHide: true,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Temporary directory is inside a Git working tree/);
});

test('a linked system temp ancestor is resolved before allocating', async (t) => {
  const root = await fixture(t);
  const actual = path.join(root, 'actual');
  const link = path.join(root, 'link');
  await mkdir(actual);
  await symlink(actual, link, process.platform === 'win32' ? 'junction' : 'dir');
  const script = `import { acquireTarget } from ${JSON.stringify(new URL('./cargo.mjs', import.meta.url).href)}; const lease = await acquireTarget(process.argv[1], {}); console.log(lease.env.TEMP); await lease.release();`;
  const result = childProcess.spawnSync(process.execPath, ['--input-type=module', '-e', script, root], {
    env: { ...process.env, TEMP: link, TMP: link, TMPDIR: link }, encoding: 'utf8', windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.ok(result.stdout.trim().startsWith(await realpath(actual)));
});

test('a forged recorded temp outside the system temp is left while its slot is reclaimed', async (t) => {
  const root = await fixture(t);
  const oldRoot = await mkdtemp(path.join(import.meta.dirname, 'faber-foreign-test-'));
  t.after(() => rm(oldRoot, { recursive: true, force: true }));
  const slot = path.join(root, 'target', 'build-1');
  const temp = path.join(oldRoot, 'faber-cargo', path.basename(tempParent(slot)), 'run-old');
  await mkdir(temp, { recursive: true });
  await writeFile(path.join(temp, 'junk'), 'junk');
  await claim(root, 1, { pid: 2147483647, childPid: 2147483646, createdAt: Date.now() - 2 * 86400000, tempName: 'run-old', tempPath: temp });
  const old = new Date(Date.now() - 2 * 86400000);
  await utimes(path.join(slot, '.faber-lease'), old, old);
  const left = t.mock.method(console, 'error', () => {});
  const lease = await launcher.acquireTarget(root, {});
  try {
    assert.equal(lease.target, slot);
    assert.equal(await readFile(path.join(temp, 'junk'), 'utf8'), 'junk');
    assert.equal(left.mock.callCount(), 1);
    assert.match(left.mock.calls[0].arguments[0], /left/);
    assert.ok(left.mock.calls[0].arguments[0].includes(JSON.stringify(temp)));
    assert.notEqual(JSON.parse(await readFile(path.join(slot, '.faber-lease', 'owner.json'))).tempPath, temp);
  } finally { await lease.release(); }
  assert.equal(existsSync(path.join(slot, '.faber-lease')), false);
});

test('a missing recorded temp releases its receipt and slot', async (t) => {
  for (const missingAt of ['absent', 'lstat', 'realpath']) {
    const root = await fixture(t);
    const slot = await claim(root, 1, { pid: 2147483647, childPid: 2147483646, createdAt: Date.now() - 2 * 86400000, tempName: 'run-missing' });
    const temp = path.join(tempParent(slot), 'run-missing');
    if (missingAt !== 'absent') await mkdir(temp, { recursive: true });
    const old = new Date(Date.now() - 2 * 86400000);
    await utimes(path.join(slot, '.faber-lease'), old, old);
    const method = missingAt === 'absent' ? 'realpath' : missingAt;
    const original = fsPromises[method];
    const missing = t.mock.method(fsPromises, method, async (folder, ...args) => {
      if (missingAt !== 'absent' && folder === temp) throw Object.assign(new Error('removed during cleanup'), { code: 'ENOENT' });
      return original(folder, ...args);
    });
    syncBuiltinESMExports();
    try {
      const lease = await launcher.acquireTarget(root, {});
      try {
        assert.equal(lease.target, slot);
        assert.notEqual(JSON.parse(await readFile(path.join(slot, '.faber-lease', 'owner.json'))).tempPath, temp);
      } finally { await lease.release(); }
      assert.equal(existsSync(path.join(slot, '.faber-lease')), false);
    } finally {
      missing.mock.restore();
      syncBuiltinESMExports();
    }
  }
});

test('an inaccessible foreign recorded temp is left while its slot is reclaimed', async (t) => {
  const root = await fixture(t);
  const temp = path.join(root, 'foreign', 'run-old');
  await mkdir(temp, { recursive: true });
  await writeFile(path.join(temp, 'keep'), 'safe');
  const old = new Date(Date.now() - 2 * 86400000);
  const slot = await claim(root, 1, { pid: 2147483647, childPid: 2147483646, createdAt: old.getTime(), tempName: 'run-old', tempPath: temp });
  await utimes(path.join(slot, '.faber-lease'), old, old);
  const original = fsPromises.realpath;
  const denied = t.mock.method(fsPromises, 'realpath', async (folder, ...args) => {
    if (folder === temp) throw Object.assign(new Error('inaccessible foreign path'), { code: 'ENOTDIR' });
    return original(folder, ...args);
  });
  syncBuiltinESMExports();
  try {
    const lease = await launcher.acquireTarget(root, {});
    try {
      assert.equal(lease.target, slot);
      assert.equal(await readFile(path.join(temp, 'keep'), 'utf8'), 'safe');
    } finally { await lease.release(); }
    assert.equal(existsSync(path.join(slot, '.faber-lease')), false);
  } finally {
    denied.mock.restore();
    syncBuiltinESMExports();
  }
});

test('a legacy receipt reaps its old temp and releases its slot', async (t) => {
  const root = await fixture(t);
  const slot = await claim(root, 1, JSON.stringify({ pid: 2147483647, childPid: 2147483646, createdAt: Date.now() - 2 * 86400000, tempName: 'run-legacy' }));
  const temp = path.join(slot, '.faber-tmp', 'run-legacy');
  await mkdir(temp, { recursive: true });
  await writeFile(path.join(temp, 'junk'), 'junk');
  const old = new Date(Date.now() - 2 * 86400000);
  await utimes(path.join(slot, '.faber-lease'), old, old);
  const lease = await launcher.acquireTarget(root, {});
  try {
    assert.equal(lease.target, slot);
    assert.equal(existsSync(path.join(slot, '.faber-tmp')), false);
  } finally { await lease.release(); }
  assert.equal(existsSync(path.join(slot, '.faber-lease')), false);
});

test('released slots are reused and never create a fourth cache', async (t) => {
  const root = await fixture(t);
  const leases = [];
  for (let i = 0; i < 3; i++) leases.push(await launcher.acquireTarget(root, {}));
  assert.deepEqual(leases.map((v) => path.basename(v.target)), ['build-1', 'build-2', 'build-3']);
  let fourth;
  const waiting = launcher.acquireTarget(root, {}).then((v) => { fourth = v; });
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.equal(fourth, undefined);
  await leases[1].release();
  await waiting;
  assert.equal(fourth.target, leases[1].target);
  await Promise.all([leases[0].release(), leases[2].release(), fourth.release()]);
  assert.deepEqual((await readdir(path.join(root, 'target'))).sort(), ['build-1', 'build-2', 'build-3']);
});

async function claim(root, index, owner) {
  const slot = path.join(root, 'target', `build-${index}`);
  const lease = path.join(slot, '.faber-lease');
  await mkdir(lease, { recursive: true });
  if (typeof owner !== 'string' && owner.tempName) owner.tempPath ??= path.join(tempParent(slot), owner.tempName);
  await writeFile(path.join(lease, 'owner.json'), typeof owner === 'string' ? owner : JSON.stringify(owner));
  return slot;
}

test('a denied reaper lock preserves the occupied cache and uses another slot', async (t) => {
  for (const code of ['EPERM', 'EACCES', 'EBUSY']) {
    const root = await fixture(t);
    const slot = await claim(root, 1, { pid: process.pid, childPid: null, createdAt: Date.now(), tempName: 'run-kept' });
    const receipt = path.join(slot, '.faber-lease', 'owner.json');
    const before = await readFile(receipt);
    const temp = path.join(tempParent(slot), 'run-kept');
    await mkdir(temp, { recursive: true });
    await writeFile(path.join(temp, 'keep'), 'owned work');
    const realMkdir = fsPromises.mkdir;
    const denied = t.mock.method(fsPromises, 'mkdir', async (folder, ...args) => {
      if (folder === path.join(slot, '.faber-reap')) throw Object.assign(new Error('simulated denied lock'), { code });
      return realMkdir(folder, ...args);
    });
    syncBuiltinESMExports();
    try {
      const next = await launcher.acquireTarget(root, {});
      try {
        assert.equal(path.basename(next.target), 'build-2');
        assert.deepEqual(await readFile(receipt), before);
        assert.equal(await readFile(path.join(temp, 'keep'), 'utf8'), 'owned work');
        assert.ok(!existsSync(path.join(slot, '.faber-reap')));
      } finally { await next.release(); }
    } finally {
      denied.mock.restore();
      syncBuiltinESMExports();
    }
  }
});

test('old live owners, live children and unreadable receipts keep their slots', async (t) => {
  const root = await fixture(t);
  const old = Date.now() - 2 * 86400000;
  const slots = [
    await claim(root, 1, { pid: process.pid, childPid: 2147483647, createdAt: old, tempName: 'run-live' }),
    await claim(root, 2, { pid: 2147483647, childPid: process.pid, createdAt: old, tempName: 'run-child' }),
    await claim(root, 3, '{bad json'),
  ];
  for (const slot of slots) await utimes(path.join(slot, '.faber-lease'), new Date(old), new Date(old));
  const controller = new AbortController();
  const waiting = launcher.acquireTarget(root, {}, controller.signal);
  const timeout = setTimeout(() => controller.abort(), 100);
  t.after(() => clearTimeout(timeout));
  await assert.rejects(waiting, /abort/i);
  for (let i = 1; i <= 3; i++) assert.ok(await readFile(path.join(root, 'target', `build-${i}`, '.faber-lease', 'owner.json')));
});

test('only a day-old claim with both processes dead reaps its own temp', async (t) => {
  const root = await fixture(t);
  const slot = await claim(root, 1, { pid: 2147483647, childPid: 2147483646, createdAt: Date.now() - 2 * 86400000, tempName: 'run-dead' });
  const temp = path.join(tempParent(slot), 'run-dead');
  await mkdir(temp, { recursive: true });
  await writeFile(path.join(temp, 'junk'), 'junk');
  const legacy = path.join(slot, '.faber-tmp', 'run-dead');
  await mkdir(legacy, { recursive: true });
  await writeFile(path.join(legacy, 'junk'), 'junk');
  const old = new Date(Date.now() - 2 * 86400000);
  await utimes(path.join(slot, '.faber-lease'), old, old);
  const lease = await launcher.acquireTarget(root, {});
  assert.equal(lease.target, slot);
  assert.ok(!(await readdir(tempParent(slot))).includes('run-dead'));
  assert.equal(existsSync(path.join(slot, '.faber-tmp')), false);
  await lease.release();
});

test('fresh or incomplete claims are not reclaimed', async (t) => {
  const root = await fixture(t);
  await claim(root, 1, { pid: 2147483647, childPid: 2147483646, createdAt: Date.now(), tempName: 'run-fresh' });
  await claim(root, 2, { pid: 2147483647, childPid: null, createdAt: Date.now() - 2 * 86400000, tempName: 'run-starting' });
  const lease = await launcher.acquireTarget(root, {});
  assert.equal(path.basename(lease.target), 'build-3');
  await lease.release();
});

test('explicit output folders and incremental settings survive without mutating caller env', async (t) => {
  const root = await fixture(t);
  const env = { CARGO_TARGET_DIR: 'custom', CARGO_INCREMENTAL: '1', TEMP: 'before' };
  const lease = await launcher.acquireTarget(root, env);
  assert.equal(lease.target, path.join(root, 'custom'));
  assert.equal(lease.env.CARGO_INCREMENTAL, '1');
  assert.equal(lease.env.TEMP, lease.env.TMP);
  assert.equal(lease.env.TMP, lease.env.TMPDIR);
  assert.equal(env.TEMP, 'before');
  await lease.release();
});

test('success and failed children clean their temporary files and preserve exit codes', async (t) => {
  const root = await fixture(t);
  for (const code of [0, 7]) {
    const result = await launcher.runChild(root, process.execPath, ['-e', 'require("node:fs").writeFileSync(require("node:path").join(process.env.TEMP,"junk"),"x");process.exit(Number(process.argv[1]))', String(code)], { ...process.env, CARGO_TARGET_DIR: '' });
    assert.equal(result, code);
    assert.deepEqual(await readdir(tempParent(path.join(root, 'target', 'build-1'))), []);
    assert.ok(!(await readdir(path.join(root, 'target', 'build-1'))).includes('.faber-lease'));
  }
});

test('linked cache roots are refused without touching their destination', async (t) => {
  const root = await fixture(t);
  const outside = await fixture(t);
  await writeFile(path.join(outside, 'keep'), 'safe');
  await symlink(outside, path.join(root, 'target'), process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(launcher.acquireTarget(root, {}), /link|real directory/i);
  assert.equal(await readFile(path.join(outside, 'keep'), 'utf8'), 'safe');
});

test('a linked abandoned temp directory is left while its slot is reclaimed', async (t) => {
  const root = await fixture(t);
  const outside = await fixture(t);
  const old = new Date(Date.now() - 2 * 86400000);
  const slot = await claim(root, 1, { pid: 2147483647, childPid: 2147483646, createdAt: old.getTime(), tempName: 'run-link' });
  await mkdir(tempParent(slot), { recursive: true });
  await writeFile(path.join(outside, 'keep'), 'safe');
  await symlink(outside, path.join(tempParent(slot), 'run-link'), process.platform === 'win32' ? 'junction' : 'dir');
  await utimes(path.join(slot, '.faber-lease'), old, old);
  const lease = await launcher.acquireTarget(root, {});
  assert.equal(lease.target, slot);
  assert.equal(await readFile(path.join(outside, 'keep'), 'utf8'), 'safe');
  assert.ok(existsSync(path.join(tempParent(slot), 'run-link')));
  await lease.release();
  assert.equal(existsSync(path.join(slot, '.faber-lease')), false);
});

test('a failed spawn releases its slot and removes its private temp', async (t) => {
  const root = await fixture(t);
  await assert.rejects(launcher.runChild(root, path.join(root, 'missing-executable'), [], {}), /ENOENT/);
  assert.deepEqual(await readdir(tempParent(path.join(root, 'target', 'build-1'))), []);
  assert.ok(!(await readdir(path.join(root, 'target', 'build-1'))).includes('.faber-lease'));
});

test('the child sees its live pid receipt and only unlinks nested links during cleanup', async (t) => {
  const root = await fixture(t);
  const outside = await fixture(t);
  await writeFile(path.join(outside, 'keep'), 'safe');
  const code = `const fs=require('node:fs'),p=require('node:path');
    const until=Date.now()+2000;
    function check(){
      const receipt=JSON.parse(fs.readFileSync(p.join(process.env.CARGO_TARGET_DIR,'.faber-lease','owner.json')));
      if(receipt.childPid!==process.pid){if(Date.now()>until)process.exit(9);setTimeout(check,10);return;}
      if(receipt.pid!==Number(process.argv[1]))process.exit(9);
      fs.symlinkSync(process.argv[2],p.join(process.env.TEMP,'link'),process.platform==='win32'?'junction':'dir');
    }check();`;
  assert.equal(await launcher.runChild(root, process.execPath, ['-e', code, String(process.pid), outside], {}), 0);
  assert.equal(await readFile(path.join(outside, 'keep'), 'utf8'), 'safe');
});

test('output-dir flags cannot bypass the leased target, while application arguments stay untouched', () => {
  for (const args of [
    ['build', '--target-dir', 'elsewhere'], ['build', '--target-dir=elsewhere'],
    ['build', '--build-dir=elsewhere'], ['build', '--config', 'build.target-dir="elsewhere"'],
    ['build', '--config=build.build-dir="elsewhere"'],
  ]) assert.throws(() => launcher.validateCargoArgs(args), /CARGO_TARGET_DIR/);
  assert.doesNotThrow(() => launcher.validateCargoArgs(['run', '--', '--target-dir=application-option']));
});

test('maintenance refuses a linked helper cache before compiling or removing anything', async (t) => {
  const root = await fixture(t);
  const outside = await fixture(t);
  await mkdir(path.join(root, 'target'));
  await writeFile(path.join(outside, 'keep'), 'safe');
  await symlink(outside, path.join(root, 'target', '.faber-tools'), process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(launcher.maintainStorage(root, path.join(root, 'target')), /link|real directory/i);
  assert.equal(await readFile(path.join(outside, 'keep'), 'utf8'), 'safe');
});

test('maintenance claims its compiler temp before spawn and cleans it after a missing compiler', async (t) => {
  const root = await fixture(t);
  const tools = path.join(root, 'target', '.faber-tools');
  await mkdir(tools, { recursive: true });
  let observed;
  // Observe the durable receipt at spawn, rather than relying on a platform's watcher notifications.
  const compiler = t.mock.method(childProcess, 'spawn', (command, args, options) => {
    observed = {
      command, env: options.env,
      owner: JSON.parse(readFileSync(path.join(tools, '.faber-lease', 'owner.json'), 'utf8')),
      tempExists: existsSync(options.env.TEMP),
    };
    const child = new EventEmitter();
    queueMicrotask(() => child.emit('error', Object.assign(new Error('simulated missing rustc'), { code: 'ENOENT' })));
    return child;
  });
  syncBuiltinESMExports();
  try {
    await assert.rejects(launcher.maintainStorage(root, path.join(root, 'target')), /rustc/);
    assert.ok(observed, 'compiler temp must have a persisted owner lease at spawn');
    assert.equal(observed.command, 'rustc');
    assert.equal(observed.owner.pid, process.pid);
    assert.equal(observed.owner.childPid, null);
    assert.equal(observed.env.CARGO_TARGET_DIR, tools);
    assert.equal(observed.env.TEMP, path.join(tempParent(tools), observed.owner.tempName));
    assert.equal(observed.tempExists, true);
    assert.deepEqual(await readdir(tempParent(tools)), []);
    assert.ok(!(await readdir(tools)).includes('.faber-lease'));
  } finally {
    compiler.mock.restore();
    syncBuiltinESMExports();
  }
});

test('maintenance publishes from the destination disk when cross-disk renames fail', async (t) => {
  const root = await fixture(t);
  const tools = path.join(root, 'target', '.faber-tools');
  const realRename = fsPromises.rename;
  const rename = t.mock.method(fsPromises, 'rename', async (from, to) => {
    if (from.startsWith(realpathSync.native(os.tmpdir()) + path.sep + 'faber-cargo')) throw Object.assign(new Error('cross-device rename'), { code: 'EXDEV' });
    return realRename(from, to);
  });
  const compiler = t.mock.method(childProcess, 'spawn', (_command, args) => {
    writeFileSync(args[args.indexOf('-o') + 1], 'not an executable');
    const child = new EventEmitter();
    queueMicrotask(() => child.emit('close', 0, null));
    return child;
  });
  syncBuiltinESMExports();
  try {
    await assert.rejects(launcher.maintainStorage(root, path.join(root, 'target')));
    assert.ok((await readdir(tools)).some((name) => name.startsWith('storage-sweep-')));
  } finally {
    rename.mock.restore();
    compiler.mock.restore();
    syncBuiltinESMExports();
  }
});

test('a nested output folder waits for its leased ancestor to release', async (t) => {
  const root = await fixture(t);
  const parent = await launcher.acquireTarget(root, { CARGO_TARGET_DIR: 'target/build-1' });
  let child;
  const waiting = launcher.acquireTarget(root, { CARGO_TARGET_DIR: 'target/build-1/nested' }).then((value) => { child = value; });
  await new Promise((resolve) => setTimeout(resolve, 40));
  assert.equal(child, undefined);
  await parent.release();
  await waiting;
  await child.release();
});

test('an ancestor output folder waits for its leased descendant to release', async (t) => {
  const root = await fixture(t);
  const child = await launcher.acquireTarget(root, { CARGO_TARGET_DIR: 'target/build-1/nested' });
  let parent;
  const waiting = launcher.acquireTarget(root, { CARGO_TARGET_DIR: 'target/build-1' }).then((value) => { parent = value; });
  await new Promise((resolve) => setTimeout(resolve, 40));
  assert.equal(parent, undefined);
  await child.release();
  await waiting;
  await parent.release();
});

test('an interrupted lease becomes uncertain so dead top-level processes cannot expose descendants to cleanup', async (t) => {
  const root = await fixture(t);
  const lease = await launcher.acquireTarget(root, {});
  await lease.child(2147483647);
  await lease.interrupt();
  const owner = JSON.parse(await readFile(path.join(lease.target, '.faber-lease', 'owner.json')));
  assert.equal(owner.childPid, null);
  await lease.release();
});

test('a successful interrupt stops the process tree, cleans temp and makes the slot reusable', async (t) => {
  const root = await fixture(t);
  const grandchildCode = 'require("node:fs").writeFileSync(require("node:path").join(process.env.TEMP,"grandchild"),String(process.pid));setInterval(()=>{},1000)';
  const childCode = 'const cp=require("node:child_process");cp.spawn(process.execPath,["-e",process.argv[1]],{stdio:"ignore",windowsHide:true});setInterval(()=>{},1000)';
  const running = launcher.runChild(root, process.execPath, ['-e', childCode, grandchildCode], {});
  const receipt = path.join(root, 'target', 'build-1', '.faber-lease', 'owner.json');
  let owner;
  let grandchild;
  const until = Date.now() + 3000;
  for (;;) {
    try {
      owner = JSON.parse(await readFile(receipt));
      grandchild = Number(await readFile(path.join(tempParent(path.join(root, 'target', 'build-1')), owner.tempName, 'grandchild')));
      break;
    } catch (error) {
      if (Date.now() > until) throw error;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  t.after(() => { for (const pid of [owner.childPid, grandchild]) { try { process.kill(pid, 'SIGKILL'); } catch { /* Already exited. */ } } });
  process.emit('SIGINT');
  assert.equal(await running, 130);
  assert.throws(() => process.kill(grandchild, 0), { code: 'ESRCH' });
  await assert.rejects(readFile(receipt), { code: 'ENOENT' });
  assert.deepEqual(await readdir(tempParent(path.join(root, 'target', 'build-1'))), []);
  const reused = await launcher.acquireTarget(root, {});
  assert.equal(path.basename(reused.target), 'build-1');
  await reused.release();
});

test('an abandoned ancestor claim cannot reap temp when a nested reservation exists', async (t) => {
  const root = await fixture(t);
  const old = new Date(Date.now() - 2 * 86400000);
  const slot = await claim(root, 1, { pid: 2147483647, childPid: 2147483646, createdAt: old.getTime(), tempName: 'run-old' });
  await mkdir(path.join(slot, '.faber-tmp', 'run-old'), { recursive: true });
  await writeFile(path.join(slot, '.faber-tmp', 'run-old', 'keep'), 'safe');
  await mkdir(path.join(slot, 'nested', '.faber-lease'), { recursive: true });
  await utimes(path.join(slot, '.faber-lease'), old, old);
  const lease = await launcher.acquireTarget(root, {});
  assert.equal(path.basename(lease.target), 'build-2');
  assert.equal(await readFile(path.join(slot, '.faber-tmp', 'run-old', 'keep'), 'utf8'), 'safe');
  assert.ok(await readFile(path.join(slot, '.faber-lease', 'owner.json')));
  await lease.release();
});

test('a missing Windows tree stopper preserves the uncertain lease and temp', { skip: process.platform !== 'win32' }, async (t) => {
  const root = await fixture(t);
  const running = launcher.runChild(root, process.execPath, ['-e', 'require("node:fs").writeFileSync(require("node:path").join(process.env.TEMP,"keep"),"x");setInterval(()=>{},1000)'], {});
  const receipt = path.join(root, 'target', 'build-1', '.faber-lease', 'owner.json');
  let owner;
  const until = Date.now() + 3000;
  for (;;) {
    try {
      owner = JSON.parse(await readFile(receipt));
      await readFile(path.join(tempParent(path.join(root, 'target', 'build-1')), owner.tempName, 'keep'));
      break;
    } catch (error) {
      if (Date.now() > until) throw error;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  t.after(() => { try { process.kill(owner.childPid, 'SIGKILL'); } catch { /* Already exited. */ } });
  const systemRoot = process.env.SystemRoot;
  try {
    process.env.SystemRoot = root;
    process.emit('SIGINT');
    assert.equal(await running, 130);
  } finally {
    if (systemRoot === undefined) delete process.env.SystemRoot;
    else process.env.SystemRoot = systemRoot;
  }
  assert.equal(JSON.parse(await readFile(receipt)).childPid, null);
  assert.equal(await readFile(path.join(tempParent(path.join(root, 'target', 'build-1')), owner.tempName, 'keep'), 'utf8'), 'x');
  const next = await launcher.acquireTarget(root, {});
  assert.equal(path.basename(next.target), 'build-2');
  await next.release();
});

test('a cache another copy built in forgets only the workspace crates, once per change of copy', async (t) => {
  const root = await fixture(t);
  const copy = async (name) => {
    const dir = path.join(root, name);
    await mkdir(path.join(dir, 'engine'), { recursive: true });
    await mkdir(path.join(dir, 'src-tauri'), { recursive: true });
    await writeFile(path.join(dir, 'Cargo.toml'), '[workspace]\nmembers = ["engine", "src-tauri"]\n');
    await writeFile(path.join(dir, 'engine', 'Cargo.toml'), '[package]\nname = "faber-engine"\n');
    await writeFile(path.join(dir, 'src-tauri', 'Cargo.toml'), '[package]\nname = "faber"\n');
    return dir;
  };
  const [a, b] = [await copy('a'), await copy('b')];
  const target = path.join(root, 'build-1');
  const own = ['debug/.fingerprint/faber-engine-0123456789abcdef', 'faber-test/debug/.fingerprint/faber-fedcba9876543210'];
  const kept = ['debug/.fingerprint/serde-0123456789abcdef', 'debug/.fingerprint/faber-engine-sys-0123456789abcdef', 'debug/deps/faber-0123456789abcdef'];
  const make = () => Promise.all([...own, ...kept].map((p) => mkdir(path.join(target, p), { recursive: true })));
  const left = () => [...own, ...kept].filter((p) => existsSync(path.join(target, p)));
  await make();
  assert.equal(await launcher.adoptTarget(target, a), true, 'a cache with no stamp may hold any copy');
  assert.deepEqual(left(), kept);
  await make();
  assert.equal(await launcher.adoptTarget(target, a), false, 'the same copy keeps its fingerprints');
  assert.deepEqual(left(), [...own, ...kept]);
  assert.equal(await launcher.adoptTarget(target, b), true, 'another copy rebuilds the workspace crates');
  assert.deepEqual(left(), kept);
  assert.equal(readFileSync(path.join(target, '.faber-copy'), 'utf8'), b);
});

test('a cache is never marked adopted when the workspace cannot be read or its stamp is a link', async (t) => {
  const root = await fixture(t);
  const target = path.join(root, 'build-1');
  await mkdir(path.join(target, 'debug', '.fingerprint', 'faber-0123456789abcdef'), { recursive: true });
  const glob = path.join(root, 'glob');
  await mkdir(glob);
  await writeFile(path.join(glob, 'Cargo.toml'), '[workspace]\nmembers = ["crates/*"]\n');
  await assert.rejects(launcher.adoptTarget(target, glob), /member pattern/);
  assert.equal(existsSync(path.join(target, '.faber-copy')), false);
  const elsewhere = path.join(root, 'elsewhere');
  await mkdir(elsewhere);
  await symlink(elsewhere, path.join(target, '.faber-copy'), 'junction');
  await assert.rejects(launcher.adoptTarget(target, glob), /linked or unusual cache stamp/);
  assert.equal(existsSync(path.join(target, 'debug', '.fingerprint', 'faber-0123456789abcdef')), true);
});
