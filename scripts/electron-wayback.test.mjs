import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { enoughSpace, readyWayback, startWayback } from '../electron/wayback.ts';

test('backs up selected data once and counts interrupted starts until ready', async () => {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'faber-wayback-'));
  try {
    mkdirSync(path.join(dir, 'conversations'));
    mkdirSync(path.join(dir, 'webview'));
    writeFileSync(path.join(dir, 'conversations', 'one.json'), 'conversation');
    writeFileSync(path.join(dir, 'settings.json'), 'settings');
    writeFileSync(path.join(dir, 'webview', 'profile.json'), 'profile');
    writeFileSync(path.join(dir, 'update.log'), 'log');
    assert.equal(enoughSpace(19, 10), false);
    assert.equal(enoughSpace(20, 10), true);
    await startWayback(dir);
    const backup = readFileSync(path.join(dir, 'backup-before-electron.done'), 'utf8').trim();
    assert.equal(readFileSync(path.join(dir, backup, 'settings.json'), 'utf8'), 'settings');
    assert.equal(readFileSync(path.join(dir, backup, 'conversations', 'one.json'), 'utf8'), 'conversation');
    writeFileSync(path.join(dir, 'settings.json'), 'changed');
    await startWayback(dir);
    await startWayback(dir);
    assert.match(readFileSync(path.join(dir, 'update.log'), 'utf8'), /failed to reach ready twice/);
    assert.equal(readdirSync(dir).filter((name) => name.startsWith('backup-before-electron-')).length, 1);
    assert.equal(readFileSync(path.join(dir, backup, 'settings.json'), 'utf8'), 'settings');
    readyWayback(dir);
    await startWayback(dir);
    assert.equal(readFileSync(path.join(dir, 'electron-starts'), 'utf8'), '1');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
import { symlinkSync } from 'node:fs';
test('a linked conversation folder never counts as a complete backup', async (t) => {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'faber-wayback-link-'));
  try {
    mkdirSync(path.join(dir, 'target'));
    try { symlinkSync(path.join(dir, 'target'), path.join(dir, 'conversations'), 'junction'); } catch { t.skip('Cannot create a directory link'); return; }
    await startWayback(dir);
    assert.equal(readdirSync(dir).includes('backup-before-electron.done'), false);
    assert.match(readFileSync(path.join(dir, 'update.log'), 'utf8'), /Backup failed/);
    assert.equal(readdirSync(dir).filter((name) => name.startsWith('backup-before-electron-')).length, 0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
test('a linked JSON file never counts as a complete backup', async (t) => {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'faber-wayback-json-link-'));
  try {
    writeFileSync(path.join(dir, 'target'), 'settings');
    try { symlinkSync(path.join(dir, 'target'), path.join(dir, 'settings.json'), 'file'); } catch { t.skip('Cannot create a file link'); return; }
    await startWayback(dir);
    assert.equal(readdirSync(dir).includes('backup-before-electron.done'), false);
    assert.match(readFileSync(path.join(dir, 'update.log'), 'utf8'), /Backup failed/);
    assert.equal(readdirSync(dir).filter((name) => name.startsWith('backup-before-electron-')).length, 0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('counter link is refused and its target remains intact', async (t) => {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'faber-wayback-counter-'));
  try {
    const target = path.join(dir, 'target');
    writeFileSync(target, 'keep');
    try { symlinkSync(target, path.join(dir, 'electron-starts'), 'file'); } catch { t.skip('Cannot create a file link'); return; }
    await startWayback(dir);
    assert.equal(readFileSync(target, 'utf8'), 'keep');
    assert.match(readFileSync(path.join(dir, 'update.log'), 'utf8'), /Startup guard failed/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
import { openSync, closeSync, ftruncateSync } from 'node:fs';
test('oversized data is skipped and logged without a partial backup', async () => {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'faber-wayback-limit-'));
  try {
    const fd = openSync(path.join(dir, 'large.json'), 'w');
    try { ftruncateSync(fd, 2 * 1024 * 1024 * 1024 + 1); } finally { closeSync(fd); }
    await startWayback(dir);
    assert.match(readFileSync(path.join(dir, 'update.log'), 'utf8'), /size limit exceeded/);
    assert.equal(readdirSync(dir).filter((name) => name.startsWith('backup-before-electron')).length, 0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
