// Integration proof: launches a hidden Electron window and serves a synthetic page on loopback only.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

test('the browser pane runs an unpacked extension, reloads its edits, and removes it', async () => {
  const scratch = await mkdtemp(path.resolve('node_modules', '.pane-integration-'));
  const electron = createRequire(import.meta.url)('electron');
  const proof = fileURLToPath(new URL('./pane-extension.proof.mjs', import.meta.url));
  try {
    const { stdout } = await promisify(execFile)(electron, [proof, scratch], { windowsHide: true, timeout: 35_000 });
    assert.match(stdout, /loaded, reloaded and removed on a real local page/);
  } finally {
    await rm(scratch, { recursive: true, force: true, maxRetries: 10 });
  }
});
