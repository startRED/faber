import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { allowedUrl, CALLS, plainProfile } from '../electron/panerules.ts';

// The shell's copy of the pane's rules answers as host/src/pane/rules.rs does (`only_web_pages_load_and_never_faber_s_own_origins`).
test('only web pages load, and never Faber\'s own origins', () => {
  for (const good of ['https://docs.rs/tokio', 'http://localhost:3000/', 'http://127.0.0.1:8080/', 'http://[::1]:3000/']) {
    assert.equal(allowedUrl(good), true, good);
  }
  for (const bad of [
    'http://tauri.localhost/',
    'https://ipc.localhost/',
    'http://x.localhost./',
    'file:///C:/',
    'tauri://localhost/',
    'faber://app/index.html',
    'faber-visual://app/0.html',
    'data:text/html,x',
    'javascript:alert(1)',
    'devtools://devtools/bundled/inspector.html',
    'chrome://gpu',
    'view-source:https://example.org/',
    'about:blank',
    'not an address',
    '',
  ]) {
    assert.equal(allowedUrl(bad), false, bad);
  }
});

test('a profile is a plain in-memory name', () => {
  assert.equal(plainProfile('faber-pane-3f2a1c9e-0000-4000-8000-000000000000'), true);
  for (const bad of ['persist:faber', '', 'a b', '../x', 'a/b', 'x'.repeat(101), 3, null, undefined]) assert.equal(plainProfile(bad), false, String(bad));
});

test('the shell runs exactly the DevTools calls the host makes', () => {
  const dir = new URL('../host/src/pane/', import.meta.url);
  const used = new Set();
  for (const file of readdirSync(dir)) {
    if (!file.endsWith('.rs') || file === 'tests.rs') continue;
    for (const [, method] of readFileSync(new URL(file, dir), 'utf8').matchAll(/"((?:Page|Network|Runtime|Emulation|Input|Target|DOM|Storage|Browser)\.[A-Za-z]+)"/g)) used.add(method);
  }
  assert.deepEqual([...CALLS].sort(), [...used].sort());
});
