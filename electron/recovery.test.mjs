import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';

test('engine restart forwards recovery to the live renderer instead of destroying its unsaved draft', () => {
  const source = stripTypeScriptTypes(readFileSync(new URL('./main.ts', import.meta.url), 'utf8'));
  const start = source.indexOf('event:', source.indexOf('host = Sidecar.start('));
  const end = source.indexOf('order:', start);
  const sent = [];
  let newerText = 'newer text whose storage write failed';
  const event = runInNewContext(`({ ${source.slice(start, end)} }).event`, {
    win: { webContents: { reload() { newerText = 'old durable text'; }, send: (...args) => sent.push(args) } },
  });
  event({ type: 'lagged' });
  assert.equal(newerText, 'newer text whose storage write failed', 'recovery must not destroy the renderer heap');
  assert.equal(sent[0]?.[0], 'faber:event');
  assert.equal(sent[0]?.[1].type, 'lagged', 'the screen must refresh engine state in place');
});
