import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { startWayback } from './wayback.ts';

test('rollback restores transcripts, project attachments and private Chat work', async () => {
  const parent = mkdtempSync(path.join(os.tmpdir(), 'faber-515b-'));
  const dir = path.join(parent, 'Faber');
  const contents = {
    'conversations/one.json': '{"id":"one"}',
    'child-transcripts/one/tool.jsonl': 'tool details',
    'attachments/one/file.txt': 'project attachment',
    'side-chats/one.json': 'side chat',
    'chat-groups/group/file.txt': 'group work',
    'chat/legacy/attachments/file.txt': 'old Chat work',
  };
  try {
    for (const [name, text] of Object.entries(contents)) {
      mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
      writeFileSync(path.join(dir, name), text);
    }
    mkdirSync(`${dir}-chat-work/one/attachments`, { recursive: true });
    writeFileSync(`${dir}-chat-work/one/attachments/file.txt`, 'private Chat attachment');
    await startWayback(dir);
    const backup = path.join(dir, readFileSync(path.join(dir, 'backup-before-electron.done'), 'utf8').trim());
    // Simulate deletion, then the documented restore into the two original roots.
    rmSync(path.join(dir, 'child-transcripts'), { recursive: true });
    rmSync(path.join(dir, 'attachments'), { recursive: true });
    rmSync(`${dir}-chat-work/one`, { recursive: true });
    cpSync(backup, dir, { recursive: true });
    cpSync(path.join(backup, 'chat-work'), `${dir}-chat-work`, { recursive: true });
    for (const [name, text] of Object.entries(contents)) assert.equal(readFileSync(path.join(dir, name), 'utf8'), text);
    assert.equal(readFileSync(`${dir}-chat-work/one/attachments/file.txt`, 'utf8'), 'private Chat attachment');
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('rollback refuses a linked private Chat root without following or marking it complete', async () => {
  const parent = mkdtempSync(path.join(os.tmpdir(), 'faber-515b-link-'));
  const dir = path.join(parent, 'Faber');
  try {
    mkdirSync(dir); mkdirSync(path.join(parent, 'outside'));
    writeFileSync(path.join(parent, 'outside', 'private.txt'), 'keep');
    symlinkSync(path.join(parent, 'outside'), `${dir}-chat-work`, 'junction');
    await startWayback(dir);
    assert.throws(() => readFileSync(path.join(dir, 'backup-before-electron.done')), { code: 'ENOENT' });
    assert.equal(readFileSync(path.join(parent, 'outside', 'private.txt'), 'utf8'), 'keep');
  } finally { rmSync(parent, { recursive: true, force: true }); }
});
