import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';

import { plan } from './cli-bin.mjs';

test('a release for one platform builds the command line for it and ships it as faber.exe', () => {
  const { args, built, shipped } = plan({ TAURI_ENV_TARGET_TRIPLE: 'x86_64-pc-windows-msvc', CARGO_TARGET_DIR: '/t' });
  assert.deepEqual(args, ['build', '-p', 'faber-cli', '--locked', '--release', '--target', 'x86_64-pc-windows-msvc']);
  assert.equal(built, path.join(path.resolve('/t'), 'x86_64-pc-windows-msvc', 'release', 'faber-cli.exe'));
  assert.equal(path.basename(shipped), 'faber.exe');
  assert.equal(path.basename(path.dirname(shipped)), 'binaries');
});

test('a debug build and a platform without .exe keep their own names', () => {
  const { args, built, shipped } = plan({ TAURI_ENV_DEBUG: 'true', TAURI_ENV_TARGET_TRIPLE: 'aarch64-apple-darwin', CARGO_TARGET_DIR: '/t' });
  assert.ok(!args.includes('--release'));
  assert.equal(built, path.join(path.resolve('/t'), 'aarch64-apple-darwin', 'debug', 'faber-cli'));
  assert.equal(path.basename(shipped), 'faber');
});

test('a target that is not a plain triple is refused', () => {
  assert.throws(() => plan({ TAURI_ENV_TARGET_TRIPLE: 'x86_64 --config evil' }), /Unexpected target/);
});
