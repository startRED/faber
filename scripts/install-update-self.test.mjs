import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, writeFileSync, unlinkSync, rmdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { EventEmitter } from 'node:events';
import { runInNewContext } from 'node:vm';
import { retained, sample, SETTINGS, packageVersion, previewVersion, NEXT, releaseVersion, builtVersion, REOPEN, WATCH, parseUpdateLog, reopenEvidence, expectReopen } from './install-update-self.mjs';

test('the resumed recorded provider streams the answer text, not only its final metadata', () => {
  const source = readFileSync(new URL('./install-update-self.mjs', import.meta.url), 'utf8');
  const template = source.match(/writeFileSync\(path\.join\(fake, 'replay\.cjs'\), (`[\s\S]*?`)\);/)[1];
  const provider = runInNewContext(template, { path, out: 'proof-output', NEXT });
  const input = new EventEmitter();
  const events = [];
  runInNewContext(provider, {
    require: (name) => {
      if (name === 'node:fs') return { existsSync: () => true, writeFileSync: () => {} };
      assert.equal(name, 'node:readline');
      return { createInterface: () => input };
    },
    process: { argv: ['node', 'replay.cjs', '-p'], stdin: {}, pid: 2 },
    console: { log: (line) => events.push(JSON.parse(line)) },
    setInterval: () => {},
  });
  input.emit('line', JSON.stringify({ type: 'user', message: { content: 'Resume' } }));
  assert.ok(events.some((event) => event.type === 'stream_event' && event.event?.delta?.type === 'text_delta' && event.event.delta.text === NEXT));
  assert.ok(events.some((event) => event.type === 'result' && event.subtype === 'success'));
});

test('dispatch versions accept only published-preview tag syntax', () => {
  assert.equal(previewVersion('v0.3.19-preview.17'), '0.3.19-preview.17');
  for (const tag of ['v1.0.0', 'v0.3.19-preview.17; exit', '../preview.17', '']) assert.throws(() => previewVersion(tag));
  const workflow = readFileSync(new URL('../.github/workflows/install-update-self.yml', import.meta.url), 'utf8');
  assert.match(workflow, /inputs\.from/);
  assert.match(workflow, /inputs\.to/);
  assert.match(workflow, /gh release download \$env:FABER_PROOF_TO --pattern latest.json/);
});

test('the installed package retains its exact preview label and rejects invalid archive ranges', () => {
  const folder = mkdtempSync(path.join(tmpdir(), 'faber-asar-version-'));
  const file = path.join(folder, 'app.asar');
  const pkg = Buffer.from(JSON.stringify({ name: 'faber', version: '0.3.18-preview.90' }));
  const header = Buffer.from(JSON.stringify({ files: { 'package.json': { offset: '0', size: pkg.length } } }));
  const size = 8 + Math.ceil(header.length / 4) * 4;
  const prefix = Buffer.alloc(8 + size);
  prefix.writeUInt32LE(4, 0);
  prefix.writeUInt32LE(size, 4);
  prefix.writeUInt32LE(size - 4, 8);
  prefix.writeUInt32LE(header.length, 12);
  header.copy(prefix, 16);
  try {
    writeFileSync(file, Buffer.concat([prefix, pkg]));
    assert.equal(packageVersion(file), '0.3.18-preview.90');
    writeFileSync(file, prefix);
    assert.throws(() => packageVersion(file));
    prefix.writeUInt32LE(0xffffffff, 12);
    writeFileSync(file, prefix);
    assert.throws(() => packageVersion(file));
  } finally { unlinkSync(file); rmdirSync(folder); }
});

test('the stored appearance bypasses first run using a real shipped theme', () => {
  const themes = JSON.parse(readFileSync(new URL('../src-tauri/src/themes.json', import.meta.url), 'utf8'));
  assert.equal(SETTINGS.appearance.base, themes.default);
  assert.equal(SETTINGS.setup, 'done');
  assert.equal(SETTINGS.tutorialDone, true);
});

test('retention requires the running-app message and setting, not just a file', () => {
  retained(sample(), { language: 'en' });
  assert.throws(() => retained({ ...sample(), messages: [] }, { language: 'en' }));
  assert.throws(() => retained(sample(), { language: 'pt-BR' }));
  assert.throws(() => retained({ ...sample(), id: 'other' }, { language: 'en' }));
});

test('three disposable Windows jobs run only on dispatch or release, retaining failures', () => {
  const workflow = readFileSync(new URL('../.github/workflows/install-update-self.yml', import.meta.url), 'utf8');
  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /release:\s+types: \[published\]/);
  assert.match(workflow, /scenario: \[tauri, clean, electron\]/);
  assert.match(workflow, /runs-on: windows-latest/);
  assert.match(workflow, /if: always\(\)/);
  assert.doesNotMatch(workflow, /schedule:|pull_request:|push:/);
  const script = readFileSync(new URL('./install-update-self.mjs', import.meta.url), 'utf8');
  assert.doesNotMatch(script, /taskkill|Stop-Process|process\.kill|remote-debugging|--inspect/);
  assert.match(script, /ui\('Update ready, restart'\)/);
  assert.match(script, /call\('conversation'/);
  assert.match(script, /conversation-open-before/);
  assert.match(script, /packageVersion\(path.join\(folder, 'resources\/app.asar'\)\)/);
  assert.doesNotMatch(script, /call\('status'\)\.version ===/);
});

// Lines as `update/src/lib.rs` writes them: `<unix seconds> <stage> <detail>`, paths already cut to `<path>`.
const log = (...lines) => lines.map((line, i) => `${1764300000 + i} ${line}\n`).join('');
const reopened = log('installed in 8123 ms', 'reopened in 912 ms');

test('update.log is read line by line, keeping only complete lines of its own shape', () => {
  const entries = parseUpdateLog(`${log('install-wait Faber was still closing; 3 of its processes were ended', 'installed in 1 ms')}\r\nnot a line\n1764300009 reopened in 4`);
  assert.deepEqual(entries.map((entry) => entry.stage), ['install-wait', 'installed']);
  assert.equal(entries[0].detail, 'Faber was still closing; 3 of its processes were ended');
  assert.equal(entries[1].at, 1764300001);
  assert.deepEqual(parseUpdateLog(''), []);
});

test('every reopen scenario needs the install, then "reopened", and no failure', () => {
  for (const scenario of ['reopen-window', 'reopen-tray']) expectReopen(scenario, reopenEvidence(parseUpdateLog(reopened)));
  // Before task 101 the helper started Faber once and wrote nothing after the install.
  assert.throws(() => expectReopen('reopen-window', reopenEvidence(parseUpdateLog(log('installed in 8123 ms')))), /reopened/);
  assert.throws(() => expectReopen('reopen-tray', reopenEvidence(parseUpdateLog(log('installed in 1 ms', 'reopen-retry Faber exited before startup was confirmed', 'reopen-failed Faber exited before startup was confirmed')))), /failed/);
  assert.throws(() => expectReopen('reopen-tray', reopenEvidence(parseUpdateLog(log('reopened in 3 ms')))), /install finished/);
  // Only what follows the newest install counts: an earlier update's reopen proves nothing about this one.
  assert.throws(() => expectReopen('reopen-tray', reopenEvidence(parseUpdateLog(`${reopened}${log('installed in 9 ms')}`))), /reopened/);
  assert.throws(() => expectReopen('reopen-tray', reopenEvidence(parseUpdateLog(log('installed in 1 ms', 'reopened in a moment')))), /reopened/);
  assert.throws(() => expectReopen('tauri', reopenEvidence(parseUpdateLog(reopened))));
});

test('a first start that exits at once must be started again', () => {
  const retried = log('installed in 1 ms', 'reopen-retry Faber exited before startup was confirmed', 'reopened in 1450 ms');
  const evidence = reopenEvidence(parseUpdateLog(retried));
  assert.deepEqual(evidence.retried, ['Faber exited before startup was confirmed']);
  expectReopen('first-start-exits', evidence);
  assert.throws(() => expectReopen('first-start-exits', reopenEvidence(parseUpdateLog(reopened))), /retried/);
  // Not a scenario: the old Faber ends about 0.3 s into the hand-off, before a proof can freeze it (run 46104395985).
  assert.throws(() => expectReopen('held-past-wait', evidence));
});

test('the built Faber is a version just below the signed release it updates to', () => {
  assert.equal(releaseVersion('v1.0.2'), '1.0.2');
  assert.equal(releaseVersion('v1.0.2-preview.30'), '1.0.2-preview.30');
  for (const tag of ['1.0.2', 'v1.0.2-alpha.0', 'v1.0.2; exit', 'v1.0', '']) assert.throws(() => releaseVersion(tag));
  // Semver orders prerelease labels by their characters: alpha comes before preview and before the release itself.
  assert.equal(builtVersion('1.0.2'), '1.0.2-alpha.0');
  assert.equal(builtVersion('1.0.2-preview.30'), '1.0.2-alpha.0');
  assert.ok('alpha' < 'preview');
});

test('the reopen jobs build Faber from the code under proof and run every new scenario', () => {
  const workflow = readFileSync(new URL('../.github/workflows/install-update-self.yml', import.meta.url), 'utf8');
  assert.ok(workflow.includes(`scenario: [${REOPEN.join(', ')}]`));
  assert.match(workflow, /npm run dist:electron/);
  assert.match(workflow, /\^\[0-9a-f\]\{40\}\$/);
  assert.match(workflow, /git checkout FETCH_HEAD -- update/);
  assert.doesNotMatch(workflow, /run: .*\$\{\{ (inputs|steps)\./);
  const desktop = readFileSync(new URL('./install-update-desktop.ps1', import.meta.url), 'utf8');
  for (const scenario of REOPEN) assert.ok(desktop.split('\n')[0].includes(`'${scenario}'`), scenario);
});

test('the lock holder is the Unicode Chromium singleton window and waits only on the install folder', () => {
  // An ANSI window procedure cut its title to one letter, so no start could find it (seen on 2026-11-28).
  const imports = WATCH.match(/\[DllImport\("user32\.dll"[^\]]*\]/g);
  assert.ok(imports.length >= 8);
  for (const line of imports) assert.match(line, /CharSet=CharSet\.Unicode/);
  assert.match(WATCH, /name = "Chrome_MessageWindow"/);
  assert.match(WATCH, /new IntPtr\(-3\)/, 'a message-only window, as Chromium makes');
  assert.match(WATCH, /file\.StartsWith\(folder \+ "\\\\"/);
  assert.match(WATCH, /!file\.StartsWith\(cli/, 'the command line the proof calls is not the old Faber');
});

test('the watched scenario serves the update only once its watcher watches', () => {
  // A watcher still compiling when the hand-off began missed it (run 46103155141).
  assert.equal(WATCH.match(/Watching\(\);/g).length, 1);
  assert.doesNotMatch(WATCH, /NtSuspendProcess|OpenProcess/, 'the proof freezes and ends nothing');
  const script = readFileSync(new URL('./install-update-self.mjs', import.meta.url), 'utf8');
  assert.match(script, /if \(!\['electron', 'first-start-exits'\]\.includes\(scenario\)\) releaseDownload\(\);/);
  assert.match(script, /watch\?\.ready\.then\(releaseDownload\);/);
});
