import test from 'node:test';
import assert from 'node:assert/strict';
import { proofsFor, withTrailer } from './ship.mjs';

test('proofs follow the changed files', () => {
  assert.deepEqual(proofsFor(['docs/a.json', 'README.md']), [['npm', 'run', 'docs']]);
  const all = proofsFor(['ui/a.ts', 'scripts/a.mjs', 'engine/a.rs']);
  assert.equal(all.length, 7);
  assert.deepEqual(all.at(-1), ['node', 'scripts/cargo.mjs', 'check', '--workspace', '--all-targets']);
  assert.equal(proofsFor(['Cargo.lock']).length, 6);
  assert.equal(proofsFor(['ui/a.ts']).length, 5);
});
test('adds a pull request trailer once', () => {
  assert.equal(withTrailer('fix: a\n', 12), 'fix: a\n\nPull-request: #12\n');
  assert.equal(withTrailer('fix: a\n\nPull-request: #3\n', 12), 'fix: a\n\nPull-request: #3\n');
  assert.equal(withTrailer('fix: a', null), 'fix: a\n');
});
