import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, writeFileSync, unlinkSync, rmdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { EventEmitter } from 'node:events';
import { runInNewContext } from 'node:vm';
import { retained, sample, SETTINGS, packageVersion, previewVersion, NEXT } from './install-update-self.mjs';

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
