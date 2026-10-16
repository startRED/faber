// node --test scripts: CI skips the build only for a change set of words (R163).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { firstCode, isWords } from './ci-scope.mjs';

test('docs/ and .md files are words, except the map a test reads', () => {
  for (const file of ['docs/roadmap.md', 'docs/research/probe.json', 'README.md', 'ui/locales/README.md', '.github/pull_request_template.md']) {
    assert.ok(isWords(file), file);
  }
  for (const file of ['docs/product-map.md', 'ui/App.tsx', 'engine/src/lib.rs', '.github/workflows/ci.yml', 'package.json', 'mdfile', 'docs.rs']) {
    assert.ok(!isWords(file), file);
  }
});

test('one file that is not words makes the whole set code, and an empty set proves nothing', () => {
  assert.equal(firstCode(['docs/roadmap.md', 'AGENTS.md']), null);
  assert.equal(firstCode(['docs/roadmap.md', 'ui/App.tsx', 'engine/src/lib.rs']), 'ui/App.tsx');
  assert.equal(firstCode(['docs/product-map.md']), 'docs/product-map.md');
  assert.equal(firstCode([]), '');
});

test('the command reads the list from its input and answers with its exit code', () => {
  const script = fileURLToPath(new URL('./ci-scope.mjs', import.meta.url));
  const run = (input) => spawnSync(process.execPath, [script], { input, encoding: 'utf8' });
  assert.equal(run('docs/roadmap.md\nAGENTS.md\n').status, 0);
  assert.equal(run('docs/roadmap.md\r\nsrc-tauri/src/main.rs\r\n').status, 3);
  assert.equal(run('').status, 3);
});
