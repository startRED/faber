import { test } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { plan } from './electron-dist.mjs';

test('packaging selects the platform and release binaries', () => {
  const windows = plan('win32', true);
  assert.equal(windows.builder, '--win');
  assert.deepEqual(windows.binaries, ['faber-sidecar.exe', 'faber-cli.exe']);
  assert.equal(path.basename(windows.target), 'electron-target');
  assert.deepEqual(windows.cargo, ['build', '--release', '-p', 'faber-sidecar', '-p', 'faber-cli', '--locked']);
  assert.equal(plan('darwin').builder, '--mac');
  assert.deepEqual(plan('darwin').triples, ['aarch64-apple-darwin', 'x86_64-apple-darwin']);
  assert.equal(plan('linux').builder, '--linux');
  assert.deepEqual(plan('linux', true).binaries, ['faber-sidecar', 'faber-cli']);
  assert.deepEqual(plan('win32', false).binaries, ['faber-sidecar.exe']);
  assert.deepEqual(plan('win32', false).cargo, ['build', '--release', '-p', 'faber-sidecar', '--locked']);
});
