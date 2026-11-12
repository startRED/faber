import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, delimiter, dirname, join, resolve } from 'node:path';
import { localeNeeded, proof, proofEnvironment, proofsFor, temporaryWorktree, withTrailer } from './ship.mjs';

test('proofs follow the changed files', () => {
  assert.deepEqual(proofsFor(['docs/a.json', 'README.md']), [['npm', 'run', 'docs']]);
  const all = proofsFor(['ui/a.ts', 'scripts/a.mjs', 'engine/a.rs']);
  assert.equal(all.length, 7);
  assert.deepEqual(all.at(-1), ['node', 'scripts/cargo.mjs', 'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings']);
  assert.equal(proofsFor(['Cargo.lock']).length, 6);
  assert.equal(proofsFor(['ui/a.ts']).length, 5);
});
test('locale proof follows English changes and removed key uses', () => {
  const en = "  'menu.open': 'Open',\n  'look.dark.name': 'Dark',";
  assert.equal(localeNeeded(['ui/locales/en.ts'], '', en), true);
  assert.equal(localeNeeded(['ui/a.ts'], "-t('menu.open')", en), true);
  assert.equal(localeNeeded(['ui/a.ts'], '-t(`look.${id}.name`)', en), true);
  assert.equal(localeNeeded(['ui/a.ts'], '-t(`${section}.${action}`)', en), true);
  assert.equal(localeNeeded(['ui/a.ts'], '-  `${section}.${action}`', en), true);
  assert.equal(localeNeeded(['ui/a.ts'], '-  ${section}.${action}', en), true);
  assert.equal(localeNeeded(['ui/a.ts'], '+t("menu.open")', en), false);
  assert.equal(localeNeeded(['ui/a.ts'], "-const prefix = 'look.da'", en), true);
  assert.equal(localeNeeded(['ui/a.ts'], '-const x = 1', en), false);
  assert.ok(proofsFor(['ui/a.ts'], true).at(-1).includes('every_english_text_is_shown_somewhere'));
});
test('adds a pull request trailer once', () => {
  assert.equal(withTrailer('fix: a\n', 12), 'fix: a\n\nPull-request: #12\n');
  assert.equal(withTrailer('fix: a\n\nPull-request: #3\n', 12), 'fix: a\n\nPull-request: #3\n');
  assert.equal(withTrailer('fix: a', null), 'fix: a\n');
});

function sandbox(t) {
  const directory = mkdtempSync(join(tmpdir(), 'faber-ship-test-'));
  t.after(() => {
    assert.equal(dirname(resolve(directory)), resolve(tmpdir()));
    rmSync(directory, { recursive: true, force: true });
  });
  return directory;
}

test('parallel deliveries share objects through separate locked, detached worktrees and remove only their own copy', (t) => {
  const directory = sandbox(t);
  const source = join(directory, 'source.git');
  const commands = [];
  const execute = (command, args, options) => {
    assert.equal(command, 'git');
    commands.push({ args, options });
    if (args[0] === 'rev-parse') return `${source}\n`;
    if (args[1] === 'config') return args.at(-1) === 'user.name' ? 'Local identity' : 'local@example.test';
    if (args[1] === 'worktree') {
      assert.equal(args[0], `--git-dir=${source}`);
      if (args[2] === 'add') {
        assert.deepEqual(args.slice(3, 7), ['--quiet', '--detach', '--no-checkout', '--lock']);
        const dir = args.at(-2);
        assert.equal(dirname(dir), directory);
        writeFileSync(join(dir, '.git'), `gitdir: ${join(source, 'worktrees', basename(dir))}\n`);
      } else {
        assert.deepEqual(args.slice(2, 5), ['remove', '--force', '--force']);
        assert.equal(dirname(args.at(-1)), directory);
        rmSync(args.at(-1), { recursive: true });
      }
      return '';
    }
    assert.equal(args[0], `--git-dir=${join(source, 'worktrees', basename(options.cwd))}`);
    assert.equal(args[1], `--work-tree=${options.cwd}`);
    return 'candidate\n';
  };
  const first = temporaryWorktree('original', directory, execute);
  const second = temporaryWorktree('original', directory, execute);
  assert.notEqual(first.dir, second.dir);
  assert.equal(first.env.GIT_AUTHOR_NAME, process.env.GIT_AUTHOR_NAME ?? 'Local identity');
  assert.equal(first.env.GIT_COMMITTER_EMAIL, process.env.GIT_COMMITTER_EMAIL ?? 'local@example.test');
  assert.equal(first.git('rev-parse', 'HEAD'), 'candidate');
  first.remove();
  assert.ok(!existsSync(first.dir));
  assert.ok(existsSync(second.dir));
  assert.equal(second.git('rev-parse', 'HEAD'), 'candidate');
  second.remove();
  assert.ok(commands.every(({ args }) => !args.some((arg) => ['clone', 'reset'].includes(arg))));
  assert.ok(commands.filter(({ args }) => args.includes('config')).every(({ args }) => args.includes('--get')));
});

test('an invalid worktree marker is cleaned up without discovering the source repository', (t) => {
  const directory = sandbox(t);
  const source = join(directory, 'source.git');
  let removed = false;
  const execute = (_, args) => {
    if (args[0] === 'rev-parse') return source;
    assert.equal(args[0], `--git-dir=${source}`);
    if (args[1] === 'config') throw Error('Identity is unset');
    assert.equal(args[1], 'worktree');
    if (args[2] === 'add') writeFileSync(join(args.at(-2), '.git'), 'broken marker\n');
    else {
      assert.equal(dirname(args.at(-1)), directory);
      rmSync(args.at(-1), { recursive: true });
      removed = true;
    }
    return '';
  };
  assert.throws(() => temporaryWorktree('original', directory, execute), /no valid Git directory/);
  assert.ok(removed);
});

test('a ship candidate numbers the fetched snapshot and proves its final tree on every retry', () => {
  const source = readFileSync(new URL('./ship.mjs', import.meta.url), 'utf8');
  assert.match(source, /'--no-write-fetch-head'.*\+refs\/heads\/phase-1-base:refs\/worktree\/faber-ship-base/);
  assert.match(source, /'scripts\/numbers\.mjs', 'assign', '--amend', '--base', base/);
  const loop = source.slice(source.indexOf('for (let attempt = 1; attempt <= 3; attempt++)'));
  assert.ok(loop.indexOf("g('merge', '--squash', original)") < loop.indexOf('proofsFor(finalFiles,'));
  assert.ok(loop.indexOf('proofsFor(finalFiles,') < loop.indexOf("g('push', remote"));
});

test('proof subprocesses cannot discover the source and can still create independent Git fixtures', () => {
  const directory = join(resolve('verification'), 'ship-unique');
  const ceiling = resolve('another-ceiling');
  const env = proofEnvironment(directory, {
    PATH: 'preserved', GIT_DIR: 'source', GIT_WORK_TREE: 'source-tree', GIT_COMMON_DIR: 'common',
    GIT_INDEX_FILE: 'source-index', GIT_CEILING_DIRECTORIES: ceiling,
  });
  assert.deepEqual(env, { PATH: 'preserved', GIT_CEILING_DIRECTORIES: `${dirname(directory)}${delimiter}${ceiling}` });
  let invoked = false;
  assert.ok(proof(['node', 'fixture-proof.mjs'], directory, env, (command, args, options) => {
    assert.equal(command, 'node');
    assert.deepEqual(args, ['fixture-proof.mjs']);
    assert.equal(options.cwd, directory);
    assert.equal(options.env, env);
    assert.equal(options.windowsHide, true);
    invoked = true;
    return { status: 0 };
  }));
  assert.ok(invoked);
});
