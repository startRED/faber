import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

const root = new URL('../', import.meta.url);
const workflow = readFileSync(new URL('.github/workflows/release.yml', root), 'utf8').replaceAll('\r\n', '\n');

test('only the explicit switch allows stable; previews ignore the switch', () => {
  const dir = mkdtempSync(join(tmpdir(), 'faber-stable-guard-'));
  try {
    mkdirSync(join(dir, 'scripts'));
    mkdirSync(join(dir, 'release'));
    for (const name of ['electron-stable-guard.mjs', 'channel.mjs']) copyFileSync(new URL(`scripts/${name}`, root), join(dir, 'scripts', name));
    const run = (tag) => spawnSync(process.execPath, [join(dir, 'scripts/electron-stable-guard.mjs'), tag], { encoding: 'utf8' }).status;
    const set = (text) => writeFileSync(join(dir, 'release/electron-stable.json'), text);
    assert.notEqual(run('v1.2.3'), 0, 'missing switch');
    assert.equal(run('v1.2.3-preview.1'), 0, 'preview without switch');
    for (const value of ['{', '{}', 'null', '{"allowed":false}', '{"allowed":"true"}', '{"allowed":true,"extra":1}']) {
      set(value);
      assert.notEqual(run('v1.2.3'), 0, value);
      assert.equal(run('v1.2.3-preview.1'), 0, value);
    }
    set('{"allowed":true}');
    assert.equal(run('v1.2.3'), 0);
    for (const tag of ['', 'v1.2', 'v1.2.3-rc.1', 'v1.2.3-preview.x']) assert.notEqual(run(tag), 0, tag);
    const committed = JSON.parse(readFileSync(new URL('release/electron-stable.json', root), 'utf8'));
    assert.deepEqual(Object.keys(committed), ['allowed']);
    assert.equal(typeof committed.allowed, 'boolean');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

function checkWorkflow(text) {
  const jobs = Object.fromEntries([...text.slice(text.indexOf('jobs:\n')).matchAll(/^  ([a-z][a-z0-9_-]*):\n([\s\S]*?)(?=^  [a-z][a-z0-9_-]*:\n|$(?![\s\S]))/gm)].map((m) => [m[1], m[2]]));
  const { create, build, publish } = jobs;
  assert.deepEqual(Object.keys(jobs), ['create', 'build', 'publish', 'checksums', 'manifest', 'main']);
  assert.ok(create && build && publish);
  assert.match(build, /^    needs: create$/m);
  assert.match(publish, /^    needs: \[create, build\]$/m);
  for (const job of [create, build, publish]) assert.doesNotMatch(job, /continue-on-error:|always\s*\(|^    if:/m);
  assert.match(publish, /make_latest=true/);
  if (/npm run dist:electron/.test(build)) {
    assert.match(create, /set -euo pipefail\n[\s\S]*?kind=\$\(node scripts\/channel\.mjs kind "\$GITHUB_REF_NAME"\)\n\s*node scripts\/electron-stable-guard\.mjs "\$GITHUB_REF_NAME"\n/);
    assert.ok(create.indexOf('node scripts/electron-stable-guard.mjs') < create.indexOf('gh api -X POST'));
    assert.doesNotMatch(create, /continue-on-error:|set \+e|\|\|\s*(true|:)|^\s+if:|\btrap\b/m);
  } else {
    assert.match(build, /uses: tauri-apps\/tauri-action@/);
    assert.doesNotMatch(text, /dist:electron|electron-builder/);
  }
}

test('Electron packaging and making a release latest depend on the guard succeeding', () => {
  checkWorkflow(workflow);
  // Exercise the check even while the current release workflow still packages Tauri.
  const guarded = /npm run dist:electron/.test(workflow) ? workflow : workflow
    .replace(/      - uses: tauri-apps\/tauri-action@[^\n]+/, '      - run: npm run dist:electron')
    .replace('kind=$(node scripts/channel.mjs kind "$GITHUB_REF_NAME")', '$&\n          node scripts/electron-stable-guard.mjs "$GITHUB_REF_NAME"');
  checkWorkflow(guarded);
  assert.throws(() => checkWorkflow(guarded.replace(/^\s*node scripts\/electron-stable-guard\.mjs "\$GITHUB_REF_NAME"\n/m, '')));
  assert.throws(() => checkWorkflow(guarded.replace('needs: create', 'needs: []')));
  assert.throws(() => checkWorkflow(guarded.replace('needs: [create, build]', 'needs: build')));
  assert.throws(() => checkWorkflow(guarded.replace('node scripts/electron-stable-guard.mjs "$GITHUB_REF_NAME"', 'node scripts/electron-stable-guard.mjs "$GITHUB_REF_NAME" || true')));
});
