// node --test scripts: the release body is built from the English entries, whichever quotes the file uses.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { body, entries } from './release-notes.mjs';

const single = "  'notes.1.new': 'Faber can work on projects.',\n  'notes.2.fixed': 'Answers recover.',\n";
const double = '  "notes.1.new": "Faber can work on projects.",\n  "notes.2.fixed": "Answers recover.",\n';

test('both quote styles give the same entries', () => {
  assert.deepEqual(entries(single), entries(double));
  assert.deepEqual(entries(single).map((e) => [e.id, e.kind]), [[1, 'new'], [2, 'fixed']]);
});

test('an escaped quote and an apostrophe inside the text survive', () => {
  assert.equal(entries("  'notes.3.better': 'It\\'s quick.',\n")[0].text, "It's quick.");
  assert.equal(entries('  "notes.3.better": "Say \\"hi\\" and it\'s done.",\n')[0].text, 'Say "hi" and it\'s done.');
});

test('only the entries newer than the previous release are listed, by kind', () => {
  const now = `${single}  'notes.3.better': 'Faster.',\n  'notes.4.new': 'A new thing.',\n`;
  assert.equal(body(now, single), '### New\n\n- A new thing.\n\n### Better\n\n- Faster.\n');
  assert.equal(body(now), body(now, ''), 'no previous release lists everything');
  assert.match(body(now), /Faber can work on projects/);
});

test('nothing new prints a plain sentence, never an empty body', () => {
  assert.equal(body(single, single), 'Small fixes and improvements.\n');
  assert.equal(body('', ''), 'Small fixes and improvements.\n');
});

test('the real English file has entries the script can read', () => {
  const real = readFileSync(new URL('../ui/locales/en.ts', import.meta.url), 'utf8');
  const found = (real.match(/^\s*['"]notes\.\d+\.(new|better|fixed)['"]:/gm) ?? []).length;
  assert.ok(found > 0);
  assert.equal(entries(real).length, found, 'every numbered entry line is read, so a reformat cannot silently empty the body');
});
