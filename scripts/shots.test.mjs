// node --test scripts: the screenshots of a visual critique (R169) are those of the screens the diff touches.

import assert from 'node:assert/strict';
import test from 'node:test';
import { LABEL, SCREENS, affected, index, keep, pick, pngSize, statusPaths } from './shots.mjs';

test('a change to one screen photographs that screen, a shared file photographs them all, the engine none', () => {
  assert.deepEqual(affected(['ui/Settings.tsx']), ['settings']);
  assert.deepEqual(affected(['ui/Welcome.tsx', 'ui\\Thread.tsx']), ['chat-home', 'project-home', 'conversation']);
  assert.deepEqual(affected(['ui/styles.css']), SCREENS);
  assert.deepEqual(affected(['ui/locales/pt-BR.ts']), SCREENS);
  assert.deepEqual(affected(['src-tauri/src/themes.json']), SCREENS);
  assert.deepEqual(affected(['engine/src/engine.rs', 'docs/design.md']), []);
});

test('only photos of this run are kept: the main screens the diff touches, and the other scenarios when asked', () => {
  const since = 1000;
  const photos = [
    { name: 'critique-settings-1920x1080.png', modified: 1500 },
    { name: 'critique-settings-1280x800.png', modified: 1600 },
    { name: 'critique-chat-home-1920x1080.png', modified: 1500 },
    { name: 'critique-conversation-1920x1080.png', modified: 900 },
    { name: 'homes-project.png', modified: 1700 },
    { name: 'ui-launch.log', modified: 1700 },
  ];
  assert.deepEqual(pick(photos, since, ['settings'], false), ['critique-settings-1280x800.png', 'critique-settings-1920x1080.png']);
  assert.deepEqual(pick(photos, since, ['settings', 'conversation'], true), [
    'critique-settings-1280x800.png',
    'critique-settings-1920x1080.png',
    'homes-project.png',
  ]);
  assert.deepEqual(pick(photos, since, [], false), []);
});

test('the size is read from the PNG header, and the index names every photo', () => {
  const png = Buffer.alloc(24);
  Buffer.from([0x89, 0x50, 0x4e, 0x47]).copy(png);
  png.writeUInt32BE(1920, 16);
  png.writeUInt32BE(1080, 20);
  assert.deepEqual(pngSize(png), { width: 1920, height: 1080 });
  assert.equal(pngSize(Buffer.from('not a png')), null);
  const text = index('after', ['settings'], [['critique-settings-1920x1080.png', { width: 1920, height: 1080 }]]);
  assert.match(text, /Screens the diff touches: settings\./);
  assert.match(text, /\| critique-settings-1920x1080\.png \| 1920x1080 \|/);
  assert.match(index('before', [], []), /touches: none/);
});

test('changed paths are read whole from git status, and a label never leaves the critique folder', () => {
  assert.deepEqual(statusPaths(' M ui/Settings.tsx\nA  scripts/shots.mjs\n?? docs/x.md\nR  ui/Old.tsx -> ui/New.tsx\n'), [
    'ui/Settings.tsx',
    'scripts/shots.mjs',
    'docs/x.md',
    'ui/New.tsx',
  ]);
  assert.ok(LABEL.test('before') && LABEL.test('after-2'));
  for (const bad of ['../../..', '..', '.', '/tmp', 'a/b', 'a\b', '-x', '']) assert.ok(!LABEL.test(bad), bad);
});

test('a diff that touches no screen, as before the first edit, keeps all four', () => {
  assert.deepEqual(keep([]), SCREENS);
  assert.deepEqual(keep(['engine/src/engine.rs']), SCREENS);
  assert.deepEqual(keep(['ui/Settings.tsx']), ['settings']);
});
