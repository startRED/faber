// node --test scripts: the release channels (R156) order versions, name previews and pick the stable build without GitHub.

import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { compare, newest, nextPreview, notesBase, parse, pickPreview, releases, stableOf, stables, userFacing } from './channel.mjs';

const conf = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));

test('only vX.Y.Z and vX.Y.Z-preview.N are release tags', () => {
  assert.deepEqual(parse('v0.3.16-preview.2'), { major: 0, minor: 3, patch: 16, preview: 2 });
  assert.deepEqual(parse('v0.3.16'), { major: 0, minor: 3, patch: 16, preview: null });
  for (const bad of ['0.3.16', 'v0.3', 'v0.3.16-rc.1', 'v0.3.16-preview', 'v0.3.16-preview.1; rm -rf', 'preview-channel', 'v1.2.3-preview.1.2']) {
    assert.equal(parse(bad), null, bad);
  }
});

test('a preview sorts below the stable it precedes and above the stable before it', () => {
  const order = ['v0.3.15', 'v0.3.16-preview.1', 'v0.3.16-preview.2', 'v0.3.16-preview.10', 'v0.3.16', 'v0.3.17-preview.1', 'v0.4.0'];
  assert.deepEqual([...order].reverse().sort(compare), order);
  assert.deepEqual(releases(['preview-channel', ...order, 'v9']), [...order].reverse());
});

test('the order matches semver, which is what the updater compares', async () => {
  const { default: semver } = await import('semver').catch(() => ({ default: null }));
  if (!semver) return; // semver only exists when node_modules holds it; the Rust test covers the same ordering
  const order = ['v0.3.15', 'v0.3.16-preview.1', 'v0.3.16-preview.10', 'v0.3.16'];
  for (let i = 0; i < order.length; i++) {
    for (let j = 0; j < order.length; j++) assert.equal(Math.sign(compare(order[i], order[j])), semver.compare(order[i].slice(1), order[j].slice(1)));
  }
});

test('the next preview counts the previews after the newest stable', () => {
  assert.equal(nextPreview(['v0.3.15']), 'v0.3.16-preview.1');
  assert.equal(nextPreview(['v0.3.15', 'v0.3.16-preview.1', 'v0.3.16-preview.2']), 'v0.3.16-preview.3');
  assert.equal(nextPreview(['v0.3.15', 'v0.3.16-preview.9', 'v0.3.16-preview.10']), 'v0.3.16-preview.11', 'a count, not a text sort');
});

test('after a promotion the numbering moves on, and old previews no longer count', () => {
  assert.equal(nextPreview(['v0.3.15', 'v0.3.16-preview.1', 'v0.3.16-preview.3', 'v0.3.16']), 'v0.3.17-preview.1');
});

test('a hand-made minor bump starts its own line of previews', () => {
  assert.equal(nextPreview(['v0.3.16', 'v0.3.16-preview.2', 'v0.4.0']), 'v0.4.1-preview.1');
  assert.equal(nextPreview(['v0.3.16', 'v0.4.0-preview.1']), 'v0.4.0-preview.2', 'previews cut for a number above the stable go on counting');
});

test('with no tag at all the first preview is v0.0.1-preview.1', () => {
  assert.equal(nextPreview([]), 'v0.0.1-preview.1');
});

test('a preview becomes the stable of the same numbers', () => {
  assert.equal(stableOf('v0.3.16-preview.4'), 'v0.3.16');
  assert.deepEqual(stables(['v0.3.16-preview.4', 'v0.3.15']), ['v0.3.15']);
});

test('the promoted preview is the newest one that is a day old, not the newest one', () => {
  const now = 1_000_000;
  const hours = (n) => now - n * 3600;
  const list = [
    { tag: 'v0.3.16-preview.1', time: hours(100) },
    { tag: 'v0.3.16-preview.2', time: hours(30) },
    { tag: 'v0.3.16-preview.3', time: hours(2) },
  ];
  assert.equal(pickPreview(list, now), 'v0.3.16-preview.2');
  assert.equal(pickPreview(list.slice(2), now), null, 'nothing is old enough yet');
  assert.equal(pickPreview([{ tag: 'v0.3.16-preview.1', time: hours(24) }], now), 'v0.3.16-preview.1', 'exactly a day counts');
  assert.equal(pickPreview([], now), null);
});

test('only feat, fix and perf commits are changes a person sees', () => {
  assert.equal(userFacing(['docs: x', 'chore: y', 'test(engine): z', 'refactor: w']), false);
  for (const subject of ['feat: a', 'fix(ui): b', 'perf: c', 'feat!: d']) assert.equal(userFacing([subject]), true, subject);
  assert.equal(userFacing(['docs: x', 'fix: y']), true);
  assert.equal(userFacing(['fixup: x', 'features: y']), false);
  assert.equal(userFacing(['']), false);
});

test('release notes: a stable starts after the previous stable, a preview after the build before it', () => {
  const all = ['v0.3.14', 'v0.3.15', 'v0.3.16-preview.1', 'v0.3.16-preview.2', 'v0.3.16'];
  assert.equal(notesBase('v0.3.16', all, () => 'never'), 'v0.3.15');
  assert.equal(notesBase('v0.3.16-preview.2', all, () => 'v0.3.16-preview.1'), 'v0.3.16-preview.1');
  assert.equal(notesBase('v0.0.1', ['v0.0.1'], () => 'never'), '', 'the first stable has no base');
});

test('the newest build is the one with the later code, and the stable on a tie', () => {
  const commits = { 'v0.3.16-preview.2': 'b', 'v0.3.16-preview.3': 'c', 'v0.3.16': 'b', 'v0.3.15': 'a' };
  const order = ['a', 'b', 'c'];
  const ancestor = (x, y) => order.indexOf(x) < order.indexOf(y);
  const pick = (...tags) => newest(tags, (tag) => commits[tag], ancestor);
  assert.equal(pick('v0.3.15', 'v0.3.16-preview.2'), 'v0.3.16-preview.2');
  assert.equal(pick('v0.3.16-preview.2', 'v0.3.16'), 'v0.3.16', 'the stable built from the same commit wins the tie');
  assert.equal(pick('v0.3.16-preview.3', 'v0.3.16'), 'v0.3.16-preview.3', 'a stable built from older code does not replace a newer preview');
  assert.equal(pick('v0.3.16', 'v0.3.16-preview.3'), 'v0.3.16-preview.3');
});

test('every bundle target accepts a pre-release version: no MSI, whose WiX wants numbers only', () => {
  const targets = conf.bundle.targets;
  assert.ok(Array.isArray(targets) && targets.length > 0, 'bundle.targets is an explicit list');
  for (const target of targets) assert.ok(!['msi', 'wix', 'all'].includes(target), `${target} would reject vX.Y.Z-preview.N`);
  assert.deepEqual([...targets].sort(), ['appimage', 'deb', 'dmg', 'nsis']);
});

// ---- the whole decision, against a real repository ----

const script = fileURLToPath(new URL('./channel.mjs', import.meta.url));

/** A repository where each step is `[subject, file, tag?, hoursAgo]`, committed in order with the given age. */
function repo(steps) {
  const dir = mkdtempSync(join(tmpdir(), 'faber-channel-'));
  const git = (...args) => execFileSync('git', args, { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  git('init', '-q', '-b', 'main');
  git('config', 'user.email', 'a@b.c');
  git('config', 'user.name', 'a');
  git('config', 'commit.gpgsign', 'false');
  git('config', 'tag.gpgsign', 'false');
  steps.forEach(([subject, file, tag, hours], i) => {
    mkdirSync(dirname(join(dir, file)), { recursive: true });
    writeFileSync(join(dir, file), String(i));
    git('add', '-A');
    const date = new Date(Date.now() - hours * 3600_000).toISOString();
    execFileSync('git', ['commit', '-q', '-m', subject], { cwd: dir, env: { ...process.env, GIT_AUTHOR_DATE: date, GIT_COMMITTER_DATE: date } });
    if (tag) git('tag', tag);
  });
  const run = (...args) => spawnSync('node', [script, ...args], { cwd: dir, encoding: 'utf8' });
  return { dir, git, run, stop: () => rmSync(dir, { recursive: true, force: true }) };
}

test('the weekly promotion takes the newest preview a day old and tags the stable of its numbers', () => {
  const r = repo([
    ['feat: first', 'ui/a.ts', 'v0.3.15', 300],
    ['feat: b', 'ui/b.ts', 'v0.3.16-preview.1', 60],
    ['fix: c', 'engine/c.rs', 'v0.3.16-preview.2', 30],
    ['feat: d', 'ui/d.ts', 'v0.3.16-preview.3', 1],
  ]);
  try {
    const out = r.run('promotion');
    assert.equal(out.status, 0, out.stderr);
    const [stable, commit] = out.stdout.trim().split(' ');
    assert.equal(stable, 'v0.3.16');
    assert.equal(commit, r.git('rev-parse', 'v0.3.16-preview.2^{commit}').trim(), 'the one a day old, not the newest');
    r.git('tag', stable, commit);
    assert.equal(r.run('next-preview').stdout.trim(), 'v0.3.17-preview.1');
    assert.equal(r.run('promotion').status, 3, 'the same week again has nothing to do');
  } finally {
    r.stop();
  }
});

test('the weekly promotion skips when nothing a person sees landed, or nothing is a day old', () => {
  const quiet = repo([
    ['feat: first', 'ui/a.ts', 'v0.3.15', 300],
    ['docs: words', 'docs/x.md', 'v0.3.16-preview.1', 60],
    ['feat: in the test runner only', 'testkit/t.rs', 'v0.3.16-preview.2', 40],
  ]);
  const fresh = repo([
    ['feat: first', 'ui/a.ts', 'v0.3.15', 300],
    ['feat: new', 'ui/b.ts', 'v0.3.16-preview.1', 2],
  ]);
  try {
    assert.equal(quiet.run('promotion').status, 3);
    assert.equal(fresh.run('promotion').status, 3);
    // An urgent fix named by hand does not wait a day.
    assert.equal(fresh.run('promotion', 'v0.3.16-preview.1').stdout.split(' ')[0], 'v0.3.16');
    assert.notEqual(fresh.run('promotion', 'v0.3.15').status, 0, 'a stable tag is not a preview');
    assert.notEqual(fresh.run('promotion', 'v0.9.9-preview.1').status, 0, 'a preview that does not exist');
  } finally {
    quiet.stop();
    fresh.stop();
  }
});

test('user-facing and notes-base read the real history', () => {
  const r = repo([
    ['feat: first', 'ui/a.ts', 'v0.3.15', 300],
    ['chore: tidy', 'docs/x.md', null, 60],
    ['fix: b', 'engine/b.rs', 'v0.3.16-preview.1', 30],
  ]);
  try {
    assert.equal(r.run('user-facing', 'v0.3.15', 'v0.3.16-preview.1').status, 0);
    assert.equal(r.run('user-facing', 'v0.3.16-preview.1', 'v0.3.16-preview.1').status, 3);
    assert.equal(r.run('notes-base', 'v0.3.16-preview.1').stdout.trim(), 'v0.3.15');
  } finally {
    r.stop();
  }
});
