import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { plan } from './cli-bin.mjs';

test('the Windows release uses the managed build and copies the right executable', () => {
  const triple = 'x86_64-pc-windows-msvc';
  const { args, target, built, shipped } = plan({ TAURI_ENV_TARGET_TRIPLE: triple, CARGO_TARGET_DIR: 'ignored' });
  assert.equal(path.basename(args[0]), 'cargo.mjs');
  assert.deepEqual(args.slice(1), ['build', '--release', '-p', 'faber-cli', '--target', triple]);
  assert.equal(built, path.join(target, triple, 'release', 'faber-cli.exe'));
  assert.equal(path.basename(shipped), 'faber.exe');
  assert.equal(path.basename(path.dirname(shipped)), 'binaries');
});

test('the native build has no target argument and rejects an invalid triple', () => {
  const { args, target, built } = plan({});
  assert.deepEqual(args.slice(1), ['build', '--release', '-p', 'faber-cli']);
  assert.equal(built, path.join(target, 'release', 'faber-cli.exe'));
  assert.throws(() => plan({ TAURI_ENV_TARGET_TRIPLE: 'bad --target evil' }), /Unexpected target/);
});
