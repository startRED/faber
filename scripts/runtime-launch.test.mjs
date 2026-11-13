import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

const launcher = path.resolve(import.meta.dirname, '../engine-projects/src/runtime-launch.py');
const python = (process.platform === 'win32' ? ['python', 'python3'] : ['python3', 'python'])
  .find(command => spawnSync(command, ['-I', '-B', '-c', 'import sys; assert sys.version_info >= (3, 10)'],
    { windowsHide: true, timeout: 5000 }).status === 0);

function fixture(t, name) {
  const root = mkdtempSync(path.join(tmpdir(), 'faber-runtime-launch-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  copyFileSync(launcher, path.join(root, 'run.py'));
  const file = (relative, text) => {
    const target = path.join(root, relative);
    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(target, text);
  };
  file('instructions/SKILL.md', `---\nname: ${name}\n---\n`);
  const run = (...args) => spawnSync(python, ['-I', '-B', path.join(root, 'run.py'), ...args], {
    encoding: 'utf8', windowsHide: true, timeout: 10000,
    env: { ...process.env, PYTHONIOENCODING: 'utf-8' },
  });
  return { root, file, run };
}

const options = { skip: python ? false : 'Python 3.10+ is required for the launcher integration checks' };

test('a runtime without ANTLR never imports an injected ANTLR or UTMOS subtree', options, t => {
  const { file, run } = fixture(t, 'faster-whisper');
  file('payload/faster_whisper/__init__.py', `
from types import SimpleNamespace
class WhisperModel:
    def __init__(self, *args, **kwargs): pass
    def transcribe(self, *args, **kwargs): return [], SimpleNamespace(language='en')
`);
  for (const relative of [
    'library/antlr4/antlr4-python3-runtime-evil/src/faster_whisper/__init__.py',
    'library/antlr4/antlr4-python3-runtime-4.9.3/src/faster_whisper/__init__.py',
    'payload/UTMOSv2-evil/faster_whisper/__init__.py',
  ]) file(relative, 'raise RuntimeError("UNVERIFIED_IMPORT_EXECUTED")\n');
  const result = run('input.wav');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).language, 'en');
});

function narration(t) {
  const setup = fixture(t, 'chatterbox-multilingual');
  setup.file('payload/soundfile.py', `
from pathlib import Path
def write(path, samples, rate): Path(path).write_text(samples, encoding='utf-8', newline='')
`);
  setup.file('payload/huggingface_hub.py', '');
  setup.file('payload/spacy_pkuseg.py', 'def pkuseg(*args, **kwargs): pass\n');
  setup.file('payload/chatterbox/__init__.py', '');
  setup.file('payload/chatterbox/mtl_tts.py', `
import antlr4
class Samples:
    def __init__(self, text): self.text = text
    def squeeze(self): return self
    def cpu(self): return self
    def numpy(self): return self.text
class ChatterboxMultilingualTTS:
    sr = 24000
    @classmethod
    def from_local(cls, *args, **kwargs): return cls()
    def generate(self, text, language_id): return Samples(text)
`);
  setup.file('library/antlr4/antlr4-python3-runtime-4.9.3/src/antlr4/__init__.py', '');
  setup.file('library/antlr4/antlr4-python3-runtime-evil/src/antlr4/__init__.py',
    'raise RuntimeError("UNVERIFIED_IMPORT_EXECUTED")\n');
  return setup;
}

test('narration uses exactly the pinned ANTLR directory and accepts a UTF-8 text file', options, t => {
  const { root, file, run } = narration(t);
  const text = 'A whole paragraph, with accents: aÃ§Ã£o.\nAnd another line.';
  file('a paragraph.txt', text);
  const output = path.join(root, 'result.wav');
  const result = run('--text-file', path.join(root, 'a paragraph.txt'), '--output', output);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(output, 'utf8'), text);
  assert.equal(JSON.parse(result.stdout).output, output);
});

test('the option separator preserves narration beginning with a hyphen', options, t => {
  const { root, run } = narration(t);
  const output = path.join(root, 'result.wav');
  const result = run('--output', output, '--', '-A paragraph beginning with a hyphen.');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(output, 'utf8'), '-A paragraph beginning with a hyphen.');
});

test('invalid narration input and output fail before importing the GPU model', options, t => {
  const { root, file, run } = fixture(t, 'chatterbox-multilingual');
  file('text.txt', 'narration');
  file('invalid-utf8.txt', Buffer.from([0xff, 0xfe]));
  mkdirSync(path.join(root, 'directory.wav'));
  for (const [args, message] of [
    [['text'], /--output is required/],
    [['text', '--output', path.join(root, 'result.mp3')], /\.wav extension/],
    [['text', '--output', path.join(root, 'missing/result.wav')], /existing directory/],
    [['text', '--output', path.join(root, 'directory.wav')], /existing directory/],
    [['text', '--text-file', path.join(root, 'text.txt')], /instead of the input argument/],
    [['--text-file', path.join(root, 'missing.txt')], /Cannot read/],
    [['--text-file', path.join(root, 'invalid-utf8.txt')], /Cannot read/],
    [['--output', path.join(root, 'result.wav')], /input argument/],
  ]) {
    const result = run(...args);
    assert.equal(result.status, 2, result.stderr);
    assert.match(result.stderr, message);
    assert.doesNotMatch(result.stderr, /ModuleNotFoundError/);
  }
});

test('audio scoring rejects the narration-only text-file option', options, t => {
  const { root, file, run } = fixture(t, 'faster-whisper');
  file('text.txt', 'text');
  const result = run('--text-file', path.join(root, 'text.txt'));
  assert.equal(result.status, 2, result.stderr);
  assert.match(result.stderr, /only for narration/);
});

test('an unwritable output directory fails before importing the GPU model', options, t => {
  const { root } = fixture(t, 'chatterbox-multilingual');
  const result = spawnSync(python, ['-I', '-B', '-c', `
import runpy, sys, tempfile
def refused(**kwargs): raise PermissionError('fixture: directory is not writable')
tempfile.TemporaryFile = refused
sys.argv = sys.argv[1:]
runpy.run_path(sys.argv[0], run_name='__main__')
`, path.join(root, 'run.py'), 'text', '--output', path.join(root, 'result.wav')], {
    encoding: 'utf8', windowsHide: true, timeout: 10000,
  });
  assert.equal(result.status, 2, result.stderr);
  assert.match(result.stderr, /output directory is not writable/);
  assert.doesNotMatch(result.stderr, /ModuleNotFoundError/);
});

test('uv launch explicitly clears inherited Python and uv overrides before allowing host variables', () => {
  const source = readFileSync(path.resolve(import.meta.dirname, '../engine-projects/src/tools/runtime.rs'), 'utf8');
  const builder = source.slice(source.indexOf('fn uv_command('), source.indexOf('fn wait_uv('));
  assert.match(builder, /\.env_clear\(\)/);
  assert.ok(builder.indexOf('.env_clear()') < builder.indexOf('process.env('), 'clear before applying the host allow-list');
});
