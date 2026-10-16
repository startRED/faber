// node --test scripts: the screen parts are dealt by the seconds of the newest green runs (R165).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { merge, parseLog } from './screen-costs.mjs';

const LOG = [
  'pass  screen  coordinator: a mission lands (97.4 s)  [temporary repos only; recorded replies (2.0 s)]',
  'pass  screen  the layout follows the width, from 720 to 1440 (11.6 s)',
  'FAIL  screen  a scenario that failed (40.0 s)',
  'pass  rust    cargo test --workspace (1609 tests) (120.6 s)',
  '1 passed, 0 failed',
].join('\r\n');

test('a run.log gives the seconds of each screen scenario that passed, and nothing else', () => {
  assert.deepEqual(
    [...parseLog(LOG)],
    [
      ['coordinator: a mission lands', 97.4],
      ['the layout follows the width, from 720 to 1440', 11.6],
    ],
  );
});

test('several runs give each scenario the middle of its seconds, rounded up', () => {
  const runs = [
    new Map([['a', 10], ['b', 2.1]]),
    new Map([['a', 30], ['b', 0.2]]),
    new Map([['a', 20], ['c', 7]]),
  ];
  assert.deepEqual(merge(runs), { a: 20, b: 3, c: 7 });
  assert.deepEqual(merge([new Map([['a', 0.4]])]), { a: 1 }, 'never zero: every scenario costs something');
});

test('the parse command prints a table testkit reads', () => {
  const dir = mkdtempSync(join(tmpdir(), 'costs-'));
  writeFileSync(join(dir, 'run.log'), LOG);
  const script = fileURLToPath(new URL('./screen-costs.mjs', import.meta.url));
  const out = JSON.parse(spawnSync(process.execPath, [script, 'parse', join(dir, 'run.log')], { encoding: 'utf8' }).stdout);
  assert.deepEqual(out.seconds, { 'coordinator: a mission lands': 98, 'the layout follows the width, from 720 to 1440': 12 });
});

test('the table in the repository has whole seconds for every scenario', () => {
  const table = JSON.parse(readFileSync(new URL('../testkit/screen-costs.json', import.meta.url), 'utf8'));
  assert.ok(Object.keys(table.seconds).length > 50);
  for (const [name, seconds] of Object.entries(table.seconds)) assert.ok(Number.isInteger(seconds) && seconds >= 1, name);
});
