// node --test scripts: a branch's placeholders get the numbers after phase-1-base's, and phase-1-base itself carries none.

import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import test from 'node:test';
import { numbered, placeholdersIn, plan, SKIP, topDecision, topNote, twiceUsed } from './numbers.mjs';

const decisions = '| R | Date |\n| :--- | :--- |\n| R160 | 2026-10-16 | A. |\n| R161 | 2026-10-16 | B. |\n';
const locale = "  'notes.14.new': 'A.',\n  \"notes.15.fixed\": \"B.\",\n";

test('the next numbers follow the highest decision and entry of the base', () => {
  assert.equal(topDecision(decisions), 161);
  assert.equal(topNote(locale), 15);
  assert.equal(topDecision(''), 0);
});

test('every placeholder of a branch is numbered, its second decision and entry after the first', () => {
  const row = '| R-next | 2026-10-17 | Amends R-next-2. |\n| R-next-2 | 2026-10-17 | C. |\n';
  const keys = "  'notes.next.new': 'X',\n  'notes.next.fixed': 'Y',\n  'notes.next-2.better': 'Z',\n";
  const code = '// The guard (R-next) keeps it.\n';
  const numbers = plan([row, keys, code], { decision: 161, note: 15 });
  assert.equal(numbered(row, numbers), '| R162 | 2026-10-17 | Amends R163. |\n| R163 | 2026-10-17 | C. |\n');
  assert.equal(numbered(keys, numbers), "  'notes.16.new': 'X',\n  'notes.16.fixed': 'Y',\n  'notes.17.better': 'Z',\n");
  assert.equal(numbered(code, numbers), '// The guard (R162) keeps it.\n');
});

test('quoted in backticks, as the docs that explain them do, the placeholders stay words', () => {
  const docs = 'Write `R-next` and `notes.next.<kind>` in a branch.\n';
  assert.deepEqual(placeholdersIn(docs), []);
  assert.equal(numbered(docs, plan([docs], { decision: 1, note: 1 })), docs);
  assert.deepEqual(placeholdersIn('a\n| R-next | x |\nb\n'), ['| R-next | x |']);
  assert.deepEqual(placeholdersIn("x:ui/locales/en.ts:9:  'notes.next.new': 'A',"), ["x:ui/locales/en.ts:9:  'notes.next.new': 'A',"]);
  assert.deepEqual(placeholdersIn('R-nextstep, SR-next, notes.nextgen'), []);
});

test('a decision number heading two rows is found', () => {
  assert.deepEqual(twiceUsed(decisions), []);
  assert.deepEqual(twiceUsed(`${decisions}| R161 | 2026-10-17 | C. |\n`), ['R161']);
});

test('docs/decisions.md uses each number once', () => {
  const root = join(import.meta.dirname, '..');
  assert.deepEqual(twiceUsed(readFileSync(join(root, 'docs/decisions.md'), 'utf8')), []);
});

// Branches carry placeholders until `node scripts/numbers.mjs assign`; the shared branches never do.
const ref = process.env.GITHUB_REF ?? '';
const onBase = ref === 'refs/heads/phase-1-base' || ref === 'refs/heads/main';

test('phase-1-base carries no placeholder', { skip: !onBase && 'only on phase-1-base and main' }, () => {
  const root = join(import.meta.dirname, '..');
  const skip = new Set(['.git', 'node_modules', 'target', 'dist']);
  const found = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      const name = relative(root, path).replaceAll('\\', '/');
      if (entry.isDirectory()) {
        if (!skip.has(entry.name)) walk(path);
      } else if (/\.(md|ts|tsx|rs|mjs|js|toml|yml|json|css|html)$/.test(entry.name) && !SKIP.includes(name)) {
        for (const line of placeholdersIn(readFileSync(path, 'utf8'))) found.push(`${name}: ${line.trim()}`);
      }
    }
  };
  walk(root);
  assert.deepEqual(found, [], 'run `node scripts/numbers.mjs assign` on the branch before it merges');
});
