import { test } from 'node:test';
import assert from 'node:assert/strict';
import { appended, line } from '../electron/startup.ts';

test('a start reads as the window, its showing and the first answer of the host', () => {
  assert.equal(line(1700000000, { window: 250, shown: 590, by: 'paint', host: 410 }), '1700000000 shown window=250ms shown=590ms by=paint host=410ms');
  assert.equal(line(1700000000, { window: 250 }), '1700000000 shown window=250ms shown=none by=none host=none');
});

test('the log keeps its newest whole lines under 32 KB', () => {
  assert.equal(appended('a\n', 'b'), 'a\nb\n');
  const full = `${'x'.repeat(100)}\n`.repeat(400);
  const kept = appended(full, 'newest');
  assert.ok(kept.length <= 32 * 1024);
  assert.ok(kept.endsWith('newest\n'));
  assert.ok(kept.startsWith('x'), 'it starts at a whole line');
});
