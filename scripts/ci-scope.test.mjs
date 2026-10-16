// node --test scripts: CI starts only the suites a change set needs, by testkit's own rules (R163).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { isWords, scope, suitesFor } from './ci-scope.mjs';

test('each path picks the suites of the table testkit tests too (testkit/src/plan.rs)', () => {
  const cases = [
    ['engine/src/store.rs', ['rust', 'screen']],
    ['src-tauri/src/main.rs', ['rust', 'ui', 'screen']],
    ['Cargo.lock', ['rust', 'ui', 'screen']],
    ['.cargo/config.toml', ['rust', 'ui', 'screen']],
    ['ui/App.tsx', ['ui', 'screen']],
    ['package.json', ['ui', 'screen']],
    ['tests/ui/i18n.check.ts', ['ui']],
    ['tests/fixtures/claude/chat.jsonl', ['rust', 'screen']],
    ['testkit/src/plan.rs', ['rust', 'ui', 'screen']],
    ['.github/workflows/ci.yml', ['rust', 'ui', 'screen']],
    ['tests/other/notes.txt', ['rust', 'ui', 'screen']],
    ['docs/product-map.md', ['rust']],
    ['docs/design.md', []],
    ['engine/NOTES.md', []],
  ];
  for (const [path, want] of cases) assert.deepEqual(suitesFor(path), want, path);
});

test('docs/ and .md files are words, except the map a test reads', () => {
  for (const file of ['docs/roadmap.md', 'docs/research/probe.json', 'README.md', 'ui/locales/README.md']) assert.ok(isWords(file), file);
  for (const file of ['docs/product-map.md', 'ui/App.tsx', 'package.json', 'mdfile', 'docs.rs']) assert.ok(!isWords(file), file);
});

test('a change set adds its suites together, words need nothing and an empty set runs everything', () => {
  assert.deepEqual(scope(['ui/App.tsx', 'README.md']), { lints: true, rust: false, ui: true, screen: true });
  assert.deepEqual(scope(['engine/src/x.rs', 'tests/ui/a.check.ts']), { lints: true, rust: true, ui: true, screen: true });
  assert.deepEqual(scope(['tests/ui/a.check.ts']), { lints: true, rust: false, ui: true, screen: false });
  assert.deepEqual(scope(['docs/roadmap.md', 'AGENTS.md']), { lints: false, rust: false, ui: false, screen: false });
  assert.deepEqual(scope([]), { lints: true, rust: true, ui: true, screen: true });
});

test('the command reads the list from its input and prints one output line per decision', () => {
  const script = fileURLToPath(new URL('./ci-scope.mjs', import.meta.url));
  const run = (input) => spawnSync(process.execPath, [script], { input, encoding: 'utf8' }).stdout;
  assert.equal(run('docs/roadmap.md\r\nui/App.tsx\r\n'), 'lints=true\nrust=false\nui=true\nscreen=true\n');
  assert.equal(run(''), 'lints=true\nrust=true\nui=true\nscreen=true\n');
});
