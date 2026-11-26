import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('the screen says it painted before it waits for the host, so the window never waits for the failsafe', () => {
  const main = readFileSync(new URL('../ui/main.tsx', import.meta.url), 'utf8');
  const told = main.indexOf('api.screenReady()');
  assert.ok(told > 0, 'the screen tells the shell it painted');
  for (const wait of ['await api.on(', 'await api.setClock(', 'await api.state()']) {
    const at = main.indexOf(wait);
    assert.ok(at > 0, `${wait} is still in ui/main.tsx`);
    assert.ok(told < at, `the paint signal comes before ${wait}`);
  }
});
