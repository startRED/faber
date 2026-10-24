import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { EventEmitter } from 'node:events';
import { existsSync, readFileSync } from 'node:fs';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm, symlink, utimes, realpath } from 'node:fs/promises';
import { syncBuiltinESMExports } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import * as launcher from './cargo.mjs';

async function fixture(t) {
  const root = await mkdtemp(path.join(await realpath(os.tmpdir()), 'faber-storage-test-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  return root;
}

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
  await writeFile(path.join(lease, 'owner.json'), typeof owner === 'string' ? owner : JSON.stringify(owner));
  return slot;
}

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
  const temp = path.join(slot, '.faber-tmp', 'run-dead');
  await mkdir(temp, { recursive: true });
  await writeFile(path.join(temp, 'junk'), 'junk');
  const old = new Date(Date.now() - 2 * 86400000);
  await utimes(path.join(slot, '.faber-lease'), old, old);
  const lease = await launcher.acquireTarget(root, {});
  assert.equal(lease.target, slot);
  assert.ok(!(await readdir(path.join(slot, '.faber-tmp'))).includes('run-dead'));
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
    assert.deepEqual(await readdir(path.join(root, 'target', 'build-1', '.faber-tmp')), []);
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

test('a linked abandoned temp directory is preserved along with its lease', async (t) => {
  const root = await fixture(t);
  const outside = await fixture(t);
  const old = new Date(Date.now() - 2 * 86400000);
  const slot = await claim(root, 1, { pid: 2147483647, childPid: 2147483646, createdAt: old.getTime(), tempName: 'run-link' });
  await mkdir(path.join(slot, '.faber-tmp'));
  await writeFile(path.join(outside, 'keep'), 'safe');
  await symlink(outside, path.join(slot, '.faber-tmp', 'run-link'), process.platform === 'win32' ? 'junction' : 'dir');
  await utimes(path.join(slot, '.faber-lease'), old, old);
  const lease = await launcher.acquireTarget(root, {});
  assert.equal(path.basename(lease.target), 'build-2');
  assert.equal(await readFile(path.join(outside, 'keep'), 'utf8'), 'safe');
  assert.ok(await readFile(path.join(slot, '.faber-lease', 'owner.json')));
  await lease.release();
});

test('a failed spawn releases its slot and removes its private temp', async (t) => {
  const root = await fixture(t);
  await assert.rejects(launcher.runChild(root, path.join(root, 'missing-executable'), [], {}), /ENOENT/);
  assert.deepEqual(await readdir(path.join(root, 'target', 'build-1', '.faber-tmp')), []);
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
    assert.equal(observed.env.TEMP, path.join(tools, '.faber-tmp', observed.owner.tempName));
    assert.equal(observed.tempExists, true);
    assert.deepEqual(await readdir(path.join(tools, '.faber-tmp')), []);
    assert.ok(!(await readdir(tools)).includes('.faber-lease'));
  } finally {
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
      grandchild = Number(await readFile(path.join(root, 'target', 'build-1', '.faber-tmp', owner.tempName, 'grandchild')));
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
  assert.deepEqual(await readdir(path.join(root, 'target', 'build-1', '.faber-tmp')), []);
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
      await readFile(path.join(root, 'target', 'build-1', '.faber-tmp', owner.tempName, 'keep'));
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
  assert.equal(await readFile(path.join(root, 'target', 'build-1', '.faber-tmp', owner.tempName, 'keep'), 'utf8'), 'x');
  const next = await launcher.acquireTarget(root, {});
  assert.equal(path.basename(next.target), 'build-2');
  await next.release();
});
