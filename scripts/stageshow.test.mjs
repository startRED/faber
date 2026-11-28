import assert from 'node:assert/strict';
import { link, mkdtemp, mkdir, readFile, readdir, realpath, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { digest, ensureDirectory, localized, options, sourcePin, validateCache, writeGenerated } from './stageshow.mjs';
import { initial, perform, poster } from '../demos/faber/scene.mjs';

test('demo accepts only bounded local-preview options', () => {
  assert.deepEqual(options([]), { language: 'en', port: 4187, mode: 'dev' });
  assert.deepEqual(options(['--lang', 'pt-BR', '--port', '4200', '--build']), { language: 'pt-BR', port: 4200, mode: 'build' });
  for (const args of [['--lang', '../en'], ['--port', '80'], ['--port', '0000'], ['--port', '1000'], ['--port', '65536'], ['--port', '4187;calc'], ['--build', '--prepare'], ['--host', '0.0.0.0'], ['--output', '../ui']]) assert.throws(() => options(args));
});

test('source comes only from the reviewed catalog pin and digest covers names and bytes', () => {
  const pin = { id: 'stageshow', repo: 'benjitaylor/skills', path: 'stageshow', commit: 'a'.repeat(40), digest: 'b'.repeat(64) };
  assert.equal(sourcePin({ skills: [pin] }), pin);
  for (const changed of [{ repo: 'somebody/skills' }, { path: '../stageshow' }, { commit: 'main' }, { digest: '' }]) assert.throws(() => sourcePin({ skills: [{ ...pin, ...changed }] }));
  const entries = [['a.txt', Buffer.from('a')], ['b.txt', Buffer.from('bc')]];
  assert.equal(digest(entries), digest([...entries].reverse()));
  assert.notEqual(digest(entries), digest([['a.txt', Buffer.from('ab')], ['b.txt', Buffer.from('c')]]));
  assert.notEqual(digest(entries), digest([['other.txt', Buffer.from('a')], entries[1]]));
});

test('a linked cache parent is refused before anything is written through it', async () => {
  const temporary = await realpath(await mkdtemp(path.join(tmpdir(), 'faber-demo-path-')));
  try {
    const external = path.join(temporary, 'other');
    const linked = path.join(temporary, 'cache');
    await mkdir(external);
    await symlink(external, linked, process.platform === 'win32' ? 'junction' : 'dir');
    await assert.rejects(ensureDirectory(path.join(linked, 'new')), /linked directories/);
    await assert.rejects(readFile(path.join(external, 'new')), { code: 'ENOENT' });
  } finally { await rm(temporary, { recursive: true, force: true }); }
});

test('generated files never overwrite an outside hard-link target', async () => {
  const temporary = await realpath(await mkdtemp(path.join(tmpdir(), 'faber-demo-output-')));
  try {
    const external = path.join(temporary, 'original.txt');
    const cache = path.join(temporary, 'demo');
    await mkdir(cache);
    await writeFile(external, 'outside work');
    for (const filename of ['index.tsx', 'content.json', 'assets.json', 'tokens.css', 'mark.svg', 'archivo.woff2', 'faber.html', 'faber.config.mjs']) {
      const destination = path.join(cache, filename);
      await link(external, destination);
      await assert.rejects(writeGenerated(destination, 'changed'), /unsafe demo destination/);
      assert.equal(await readFile(external, 'utf8'), 'outside work');
      await rm(destination);
    }
    const regular = path.join(cache, 'scene.mjs');
    await writeGenerated(regular, 'first');
    await writeGenerated(regular, 'second');
    assert.equal(await readFile(regular, 'utf8'), 'second');
    assert.deepEqual(await readdir(cache), ['scene.mjs']);
  } finally { await rm(temporary, { recursive: true, force: true }); }
});

test('untracked executable config is refused before a cached tool can run', async () => {
  const temporary = await realpath(await mkdtemp(path.join(tmpdir(), 'faber-demo-source-')));
  try {
    await mkdir(path.join(temporary, 'stageshow/playground'), { recursive: true });
    await writeFile(path.join(temporary, 'stageshow/playground/index.html'), 'pinned');
    const tracked = ['stageshow/playground/index.html'];
    await validateCache(temporary, tracked);
    await mkdir(path.join(temporary, 'stageshow/demo-public/notices'), { recursive: true });
    await writeFile(path.join(temporary, 'stageshow/demo-public/notices/archivo-OFL.txt'), 'SIL Open Font License');
    await validateCache(temporary, tracked);
    for (const config of ['stageshow/playground/postcss.config.cjs', 'stageshow/.npmrc', 'stageshow/.babelrc', 'tsconfig.json', '.gitignore']) {
      const filename = path.join(temporary, config);
      await writeFile(filename, 'unreviewed executable config');
      let ran = false;
      await assert.rejects(async () => { await validateCache(temporary, tracked); ran = true; }, /Unexpected demo source entry/);
      assert.equal(ran, false);
      await rm(filename);
    }
  } finally { await rm(temporary, { recursive: true, force: true }); }
});

test('build output refuses a junction and a hard-linked asset without changing their outside target', async () => {
  const temporary = await realpath(await mkdtemp(path.join(tmpdir(), 'faber-demo-build-')));
  try {
    const external = path.join(temporary, 'outside');
    const checkout = path.join(temporary, 'checkout');
    await mkdir(external);
    await mkdir(path.join(checkout, 'stageshow'), { recursive: true });
    const output = path.join(checkout, 'stageshow/dist-faber');
    await symlink(external, output, process.platform === 'win32' ? 'junction' : 'dir');
    await assert.rejects(validateCache(checkout, []), /Linked demo source entry/);
    await rm(output);
    await mkdir(output);
    const assets = path.join(output, 'assets');
    await symlink(external, assets, process.platform === 'win32' ? 'junction' : 'dir');
    await assert.rejects(validateCache(checkout, []), /Linked demo source entry/);
    await rm(assets);
    const original = path.join(external, 'index.html');
    await writeFile(original, 'outside result');
    await link(original, path.join(output, 'index.html'));
    await assert.rejects(validateCache(checkout, []), /Hard-linked demo source entry/);
    assert.equal(await readFile(original, 'utf8'), 'outside result');
  } finally { await rm(temporary, { recursive: true, force: true }); }
});

test('every demo language supplies real text and synthetic disclosure from Faber locales', async () => {
  for (const [language, exportName] of [['en', 'en'], ['pt-BR', 'ptBR'], ['es', 'es'], ['fr', 'fr'], ['de', 'de']]) {
    const module = await import(`../ui/locales/${language}.ts`);
    const content = localized(module[exportName], language);
    assert.equal(content.language, language);
    assert.equal(content.brand, 'Faber');
    for (const text of Object.values(content)) assert.equal(typeof text, 'string');
    assert.ok(content.synthetic.length > 5);
    const incomplete = { ...module[exportName], 'demo.faber.synthetic': '' };
    assert.throws(() => localized(incomplete, language), /demo.faber.synthetic/);
  }
});

test('the actual story sends the request, works, checks, then reveals a persistent result', async () => {
  const state = { ...initial };
  const steps = [];
  let waited = 0;
  await perform({
    mark(name) { steps.push(name); },
    async wait(ms) { waited += ms; },
    async enter() { assert.fail('The cursor already rests on the field; entering would jump it offscreen'); },
    async move(target) { steps.push(`move:${target}`); },
    async click(target) { steps.push(`click:${typeof target === 'string' ? target : target.at}`); },
    async type(key, text) { state[key] = text; steps.push('typed'); },
    async set(patch) {
      if (patch.sent) assert.ok(state.draft);
      if (patch.result) assert.equal(state.phase, 3);
      if (patch.preview) assert.equal(state.result, true);
      Object.assign(state, patch);
    },
  }, 'Build my weekend planner');
  assert.deepEqual(steps, ['request', 'move:request', 'click:request', 'typed', 'click:send', 'work', 'result', 'click:preview', 'review']);
  assert.deepEqual(state, poster);
  assert.ok(waited >= 10000 && waited <= 14000, 'The short story leaves reading time for its result');
  const css = await readFile(new URL('../demos/faber/faber.css', import.meta.url), 'utf8');
  assert.match(css, /prefers-reduced-motion: reduce/);
  const scene = await readFile(new URL('../demos/faber/index.tsx', import.meta.url), 'utf8');
  assert.match(scene, /compact=\{\{ below: 600, width: 390/);
  assert.match(scene, /poster=\{poster\}/);
  assert.match(scene, /play="once"/);
  assert.match(scene, /c\.synthetic/);
  const config = await readFile(new URL('../demos/faber/vite.config.mjs', import.meta.url), 'utf8');
  assert.match(config, /esbuild: \{ jsx: 'automatic', jsxImportSource: 'react' \}/, 'The separate React demo must not inherit Faber’s Solid JSX runtime');
  assert.match(config, /publicDir: '\.\.\/demo-public'/, 'Standalone exports must carry the generated font and runtime license notices');
});

