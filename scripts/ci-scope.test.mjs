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

test('main warms lint/app caches; Rust suites refresh their cache only when they actually run (R165)', () => {
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
  assert.equal(read.length, 5, read.join(', '));
  assert.deepEqual([...new Set(warmed)].sort(), [...new Set(read.filter((key) => !key.endsWith(' rust')))].sort());
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

test('a runner GitHub never provided retries once and never blames riders (41518314073)', async () => {
  const ci = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8').replaceAll('\r\n', '\n');
  const script = (section) => {
    const lines = section.split('          script: |\n')[1].split('\n');
    const end = lines.findIndex((line) => line.trim() && !line.startsWith('            '));
    return lines.slice(0, end < 0 ? lines.length : end).map((line) => line.slice(12)).join('\n');
  };
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  const detect = new AsyncFunction('github', 'context', 'core', script(ci.split('\n  gate:')[1]));
  const retry = new AsyncFunction('github', 'context', 'core', 'process', script(ci.split('\n  report:')[1]));
  const context = { repo: { owner: 'owner', repo: 'repo' }, runId: 1, ref: 'refs/heads/queue/pr-1' };
  let unavailable, dispatched;
  const summaries = [];
  const core = {
    setOutput: (_, value) => { unavailable = value; }, warning: () => {},
    summary: { addRaw: (value) => { summaries.push(value); return { write: async () => {} }; } },
  };
  const jobs = [{ name: 'check (macos-latest, lints rust ui)', runner_id: 0, steps: [],
    conclusion: 'cancelled', check_run_url: 'https://api.github.com/repos/owner/repo/check-runs/42' }];
  let annotations = [{ message: 'The job was not acquired by Runner of type hosted even after multiple attempts' }];
  const github = {
    rest: { actions: { listJobsForWorkflowRun: 'jobs', createWorkflowDispatch: async (args) => { dispatched = args; } },
      checks: { listAnnotations: 'annotations' } },
    paginate: async (route) => route === 'jobs' ? jobs : annotations,
  };
  await detect(github, context, core);
  assert.equal(unavailable, 'true', 'actual cancelled/runner_id=0 incident must not blame code');
  for (const name of ['changes', 'docs', 'costs', 'app', 'ui', 'screen (1/8)']) {
    jobs[0].name = name;
    await detect(github, context, core);
    assert.equal(unavailable, 'true', name);
  }
  for (const name of ['electron-app', 'electron']) {
    jobs[0].name = name;
    await detect(github, context, core);
    assert.equal(unavailable, 'false', 'advisory Electron jobs cannot fail the gate or blame riders');
  }
  assert.match(ci, /ELECTRON_BLOCKS_GATE: 'false'/);
  jobs[0].name = 'check (macos-latest, lints rust ui)';
  await retry(github, context, core, { env: { GITHUB_RUN_ATTEMPT: '1' } });
  assert.deepEqual(dispatched, { ...context.repo, workflow_id: 'ci.yml', ref: 'queue/pr-1', inputs: { runner_retry: 'true' } });
  dispatched = undefined;
  await retry(github, context, core, { env: { RUNNER_RETRY: 'true', GITHUB_RUN_ATTEMPT: '1' } });
  assert.equal(dispatched, undefined, 'capacity outages must not loop');
  await retry(github, context, core, { env: { GITHUB_RUN_ATTEMPT: '2' } });
  assert.equal(dispatched, undefined, 'manual reruns must not reset the retry budget');
  assert.match(summaries.join('\n'), /Train held for infrastructure; no rider is blamed/);
  annotations = [{ message: 'Due to capacity constraints, jobs targeting macOS arm64 runners may experience longer queue times.' }];
  await detect(github, context, core);
  assert.equal(unavailable, 'false', 'a capacity notice alone is not proof of an unacquired job');
  jobs[0].runner_id = 7;
  jobs[0].steps = [{ name: 'cargo t', conclusion: 'failure' }];
  annotations = [{ message: 'The job was not acquired by Runner of type hosted even after multiple attempts' }];
  await detect(github, context, core);
  assert.equal(unavailable, 'false', 'started code failures still blame/split normally');
  github.paginate = async () => { throw new Error('API unavailable'); };
  await detect(github, context, core);
  assert.equal(unavailable, 'true', 'unreadable infrastructure evidence must not blame riders');
  assert.match(ci, /RUNNER_UNAVAILABLE.*steps\.runner\.outputs\.unavailable/);
  assert.match(ci, /\[ "\$RUNNER_UNAVAILABLE" != true \] \|\| .*exit 1/);
  const report = ci.split('\n  report:')[1].split('\n  train:')[0];
  assert.equal(report.match(/if: needs\.gate\.outputs\.runner_unavailable != 'true'/g).length, 3,
    'checkout, setup and blame/split are skipped on infrastructure failure');
});
