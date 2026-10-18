// node --test scripts: a pull request lands as one commit named after it, and the helper says why one cannot.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { covered, statusText, toCheck } from './ci-scope.mjs';
import { conflicts, insertions, message, noCheck, refusal, settle, WORDS } from './land.mjs';

test('the commit is the title with its number, then the body without the template comments', () => {
  const body = '## What changed\n\n<!-- One or two sentences. -->\nFragments.   \n\n\n\n## Cost\n\n<!-- Memory.\n     More. -->\nNone.\n';
  assert.equal(
    message('ci: land through the queue (R-next)', 63, body, 'abc123'),
    'ci: land through the queue (R-next) (#63)\n\n## What changed\n\nFragments.\n\n## Cost\n\nNone.\n\nPull-request-head: abc123\n',
  );
  assert.equal(message('fix: x', 1, '', 'abc'), 'fix: x (#1)\n\nPull-request-head: abc\n');
  assert.equal(message('fix: x', 1, null, 'abc'), 'fix: x (#1)\n\nPull-request-head: abc\n');
});

test('a conflict names each file once', () => {
  assert.deepEqual(conflicts('4b825dc\nui/locales/en.ts\ndocs/decisions.md\nui/locales/en.ts\n'), ['ui/locales/en.ts', 'docs/decisions.md']);
  assert.deepEqual(conflicts('4b825dc\n'), []);
});

test('only an open pull request ready for review and aimed at phase-1-base lands', () => {
  const ready = { state: 'OPEN', isDraft: false, baseRefName: 'phase-1-base', isCrossRepository: false };
  assert.equal(refusal(ready), '');
  assert.equal(refusal({ ...ready, isDraft: true }), 'it is a draft');
  assert.equal(refusal({ ...ready, state: 'MERGED' }), 'it is merged');
  assert.equal(refusal({ ...ready, baseRefName: 'main' }), 'it targets main, not phase-1-base');
  assert.equal(refusal({ ...ready, isCrossRepository: true }), 'it comes from a fork');
});

test('rows both sides added at one place are kept, the base first; an edit both sides made is not settled', () => {
  const added = '| a |\n<<<<<<< base\n| z |\n||||||| 1234\n=======\n| c |\n| d |\n>>>>>>> pr\nend\n';
  assert.equal(insertions(added), '| a |\n| z |\n| c |\n| d |\nend\n');
  assert.equal(insertions('no conflict\n'), 'no conflict\n');
  const edited = '<<<<<<< base\n| b2 |\n||||||| 1234\n| b |\n=======\n| b3 |\n>>>>>>> pr\n';
  assert.equal(insertions(edited), null);
  assert.equal(insertions('<<<<<<< base\n| z |\n=======\n| c |\n>>>>>>> pr\n'), null, 'without diff3 markers nothing is known');
  assert.ok(WORDS.test('docs/product-map.md') && WORDS.test('AGENTS.md') && !WORDS.test('ui/locales/en.ts'));
});

test('CRLF markers are read as markers and the lines keep their CRLF', () => {
  const crlf = '| a |\r\n<<<<<<< base\r\n| z |\r\n||||||| 1234\r\n=======\r\n| c |\r\n>>>>>>> pr\r\nend\r\n';
  assert.equal(insertions(crlf), '| a |\r\n| z |\r\n| c |\r\nend\r\n');
});

test('only content conflicts in files of words are settled; any other record refuses the landing', () => {
  const head = 'abc123\ndocs/a.md\n\n';
  const odd = `${head}Auto-merging docs/a.md\nCONFLICT (content): Merge conflict in docs/a.md\nCONFLICT (file location): docs/a.md added in pr inside a directory that was renamed\n`;
  assert.ok(settle(odd).refused.some((r) => r.startsWith('(file location)')));
  assert.deepEqual(settle('abc123\nui/x.ts\n\nCONFLICT (content): Merge conflict in ui/x.ts\n').refused, ['ui/x.ts']);
  assert.deepEqual(settle('abc123\n\nCONFLICT (directory rename split): unclear\n').refused, ['(directory rename split): unclear']);
});

test('before it queues, the PC checks the suites CI would run, and nothing where it cannot or need not (R190 V2)', () => {
  assert.deepEqual(toCheck(['engine/src/x.rs', 'ui/App.tsx']), ['rust', 'ui', 'screen']);
  assert.deepEqual(toCheck(['tests/ui/a.check.ts']), ['ui'], 'the lints aside: CI runs them');
  assert.deepEqual(toCheck(['docs/roadmap.md']), [], 'words need no suite');
  assert.equal(noCheck({}, ['63'], 'win32'), '');
  assert.equal(noCheck({}, ['63', '--no-verify'], 'win32'), '--no-verify');
  assert.equal(noCheck({}, ['63', '--dry-run'], 'win32'), 'a dry run');
  assert.equal(noCheck({ GITHUB_ACTIONS: 'true' }, ['63', '--head', 'abc'], 'win32'), 'it runs on CI', "the land job's run again on a new base");
  assert.equal(noCheck({}, ['63'], 'darwin'), 'the suites run whole only on Windows', 'elsewhere the screen suite skips and exits 0');
});

test('a green faber/verify of the very tree leaves out the suites it covers, and nothing else does (R190 V2)', () => {
  const tree = 'a'.repeat(40);
  assert.equal(statusText(tree, ['rust', 'screen']), `tree ${tree} covers rust screen`);
  assert.deepEqual(covered(statusText(tree, ['rust', 'ui', 'screen']), tree), ['rust', 'ui', 'screen']);
  assert.deepEqual(covered(statusText(tree, ['rust']), 'b'.repeat(40)), [], 'another tree: the squash moved');
  for (const odd of [undefined, '', `tree ${tree} covers`, `tree ${tree} covers lints`, `tree ${tree} covers rust gpu`, ` tree ${tree} covers rust`, `tree ${tree.toUpperCase()} covers rust`]) {
    assert.deepEqual(covered(odd, tree), [], String(odd));
  }
  const script = fileURLToPath(new URL('./ci-scope.mjs', import.meta.url));
  const run = (input, env) => spawnSync(process.execPath, [script], { input, encoding: 'utf8', env: { ...process.env, ...env } }).stdout;
  const all = 'engine/src/x.rs\nui/App.tsx\n';
  assert.equal(run(all, {}), 'lints=true\nrust=true\nui=true\nscreen=true\n', 'a tree without it runs as today');
  assert.equal(run(all, { FABER_VERIFY: statusText(tree, ['rust', 'ui', 'screen']), TREE: tree }), 'lints=true\nrust=false\nui=false\nscreen=false\n', 'only the lints');
  assert.equal(run(all, { FABER_VERIFY: statusText(tree, ['rust']), TREE: tree }), 'lints=true\nrust=false\nui=true\nscreen=true\n');
  assert.equal(run(all, { FABER_VERIFY: statusText(tree, ['rust', 'ui', 'screen']), TREE: 'c'.repeat(40) }), 'lints=true\nrust=true\nui=true\nscreen=true\n');
  // The changes job counts only a success the repository's owner posted on the queued commit, and only on a queue branch.
  const ci = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
  assert.match(ci, /\[\.\[\] \| select\(\.context == \\"faber\/verify\\" and \.creator\.login == \\"\$OWNER\\"\)\]\[0\] \| select\(\.state == \\"success\\"\)/, "the owner's newest, and only if it is a success");
  assert.match(ci, /OWNER: \$\{\{ github\.repository_owner \}\}/);
  assert.match(ci, /FABER_VERIFY="\$verify" TREE="\$tree" node scripts\/ci-scope\.mjs/);
});

test("CI's own run of land.mjs, copied alone, loads without the files beside it", () => {
  const dir = mkdtempSync(join(tmpdir(), 'faber-land-alone-'));
  try {
    copyFileSync(fileURLToPath(new URL('./land.mjs', import.meta.url)), join(dir, 'land.mjs'));
    const out = spawnSync(process.execPath, [join(dir, 'land.mjs')], { encoding: 'utf8' });
    assert.equal(out.status, 2, out.stderr);
    assert.match(out.stderr, /^usage: /);
    // Run through a link to its folder, as on macOS, whose temporary folder /var is a link to /private/var.
    symlinkSync(dir, `${dir}-link`, 'junction');
    const linked = spawnSync(process.execPath, [join(`${dir}-link`, 'land.mjs')], { encoding: 'utf8' });
    assert.equal(linked.status, 2, linked.stderr);
  } finally {
    rmSync(`${dir}-link`, { force: true });
    rmSync(dir, { recursive: true, force: true });
  }
});
