// node --test scripts: CI starts only the suites a change set needs, by testkit's own rules (R163).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
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
    ['engine/tests/codex_session.rs', ['rust']],
    ['src-tauri/tests/icons.rs', ['rust']],
    ['engine/Cargo.toml', ['rust', 'ui', 'screen']],
    ['testkit/src/plan.rs', ['rust', 'ui', 'screen']],
    ['.github/workflows/ci.yml', ['rust', 'ui', 'screen']],
    ['tests/other/notes.txt', ['rust', 'ui', 'screen']],
    ['docs/product-map.md', ['rust']],
    ['docs/design.md', []],
    ['engine/NOTES.md', []],
    ['scripts/land.mjs', []],
    ['scripts/land.test.mjs', []],
    ['scripts/ci-scope.mjs', ['rust', 'ui', 'screen']],
    ['scripts/numbers.mjs', ['rust', 'ui', 'screen']],
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

test('the queue and release scripts and the scripts tests run the lints only, which run those tests', () => {
  for (const file of ['scripts/land.mjs', 'scripts/tasks.mjs', 'scripts/channel.mjs', 'scripts/pace.mjs', 'scripts/direction.mjs', 'scripts/bench/run.test.mjs']) {
    assert.ok(!isWords(file), file);
    assert.deepEqual(scope([file, 'docs/dev/workflow.md']), { lints: true, rust: false, ui: false, screen: false }, file);
  }
  // Every scripts test the lints job runs is in the rule, and no script a suite or the runner reads is.
  const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
  for (const file of pkg.scripts['test:scripts'].split(' ').filter((w) => w.endsWith('.test.mjs'))) {
    assert.deepEqual(suitesFor(file), [], file);
  }
  for (const file of ['scripts/base.mjs', 'scripts/owed.mjs', 'scripts/ci-scope.mjs', 'scripts/docs.mjs', 'scripts/numbers.mjs', 'scripts/release-notes.mjs', 'scripts/screen-costs.mjs', 'scripts/shots.mjs', 'scripts/woff2-only.mjs', 'scripts/first-user.mjs', 'scripts/cargo.mjs', 'scripts/persona-eval.mjs']) {
    assert.deepEqual(suitesFor(file), ['rust', 'ui', 'screen'], file);
  }
});

test('the command reads the list from its input and prints one output line per decision', () => {
  const script = fileURLToPath(new URL('./ci-scope.mjs', import.meta.url));
  const run = (input) => spawnSync(process.execPath, [script], { input, encoding: 'utf8' }).stdout;
  assert.equal(run('docs/roadmap.md\r\nui/App.tsx\r\n'), 'lints=true\nrust=false\nui=true\nscreen=true\n');
  assert.equal(run(''), 'lints=true\nrust=true\nui=true\nscreen=true\n');
});

test('main warms every Rust cache the checks read, under the same names, and only main saves (R165)', () => {
  // A Windows checkout may end its lines with CRLF.
  const ci = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
  const warm = ci.slice(ci.indexOf('\n  warm:'), ci.indexOf('\n  main:'));
  const warmed = [...warm.matchAll(/\{ os: ([a-z-]+), cache: ([a-z-]+) \}/g)].map((m) => `${m[1]} ${m[2]}`);
  // The check entries are JSON in the changes job; the macOS lints entry is built from a shell variable.
  const read = [
    ...[...ci.matchAll(/"os":"([a-z-]+)","part":"[^"]*","cache":"([a-z-]+)"/g)].map((m) => `${m[1]} ${m[2]}`),
    ...[...ci.matchAll(/\\"os\\":\\"([a-z-]+)\\",\\"part\\":\\"\$part\\",\\"cache\\":\\"([a-z-]+)\\"/g)].map((m) => `${m[1]} ${m[2]}`),
    ...(/\n {2}app:[\s\S]*?shared-key: app\n/.test(ci) ? ['windows-latest app'] : []),
    ...(/\n {2}electron-app:[\s\S]*?shared-key: electron-app\n/.test(ci) ? ['windows-latest electron-app'] : []),
  ];
  assert.equal(read.length, 6, read.join(', '));
  assert.deepEqual([...new Set(warmed)].sort(), [...new Set(read)].sort());
  assert.equal(ci.match(/uses: Swatinem\/rust-cache@v2/g).length, 4, 'check, app, electron-app and warm');
  assert.equal(ci.match(/save-if: \$\{\{ github\.ref == 'refs\/heads\/main' \}\}/g).length, 3, 'check, app and electron-app save only on main');
});

test('Windows COM updates stay on the minor line required by WebView2', () => {
  const config = readFileSync(new URL('../.github/dependabot.yml', import.meta.url), 'utf8').replaceAll('\r\n', '\n');
  const cargo = config.split('  - package-ecosystem: cargo\n')[1]?.split('\n  - package-ecosystem:')[0];
  assert.ok(cargo, 'Cargo dependency updates must remain configured');
  assert.match(cargo, /dependency-name: windows-core\s+versions: \['>=0\.63'\]/,
    'Keep 0.62 patch updates; a later minor must wait for WebView2 to use matching COM types');
  const manifest = readFileSync(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8');
  assert.match(manifest, /windows-core = "0\.62"/);
  assert.match(manifest, /webview2-com = "0\.39"/);
});
