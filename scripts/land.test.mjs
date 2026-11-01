// node --test scripts: a pull request lands as one commit named after it, and the helper says why one cannot.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { covered, isWords, statusText, toCheck } from './ci-scope.mjs';
import { areas, BATCH, conflicts, dropped, eligibleStack, half, redAt, riders, running, TRAIN, landCancelled, insertions, landsAsChecked, message, noCheck, refusal, settle, stacked, stackOn, WORDS, wordsOnly, workflow } from './land.mjs';

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

test('an addition already included by another lane is kept once without losing later additions', () => {
  const conflict = (left, right) => `before\n<<<<<<< base\n${left}\n||||||| old\n=======\n${right}\n>>>>>>> pr\nafter\n`;
  const paragraph = 'The runner checks the invoked workspace.\nIts cached programs stay reusable.';
  const combined = `${paragraph}\n\nRecorded sessions stay isolated.`;
  assert.equal(insertions(conflict(combined, paragraph)), `before\n${combined}\nafter\n`);
  assert.equal(insertions(conflict(paragraph, combined)), `before\n${combined}\nafter\n`);
  assert.equal(insertions(conflict(paragraph, paragraph)), `before\n${paragraph}\nafter\n`);
  assert.equal(insertions(conflict('A complete line.', 'complete line.')), 'before\nA complete line.\ncomplete line.\nafter\n', 'a substring is not a whole added line');
  const crlf = conflict(combined, paragraph).replace(/\n/g, '\r\n');
  assert.equal(insertions(crlf), `before\n${combined}\nafter\n`.replace(/\n/g, '\r\n'));
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

test('the parts a change touches leave out the words, the locales and the docs every landing writes', () => {
  assert.deepEqual([...areas(['engine/src/a.rs', 'engine/tests/b.rs', 'ui/Thread.tsx', 'ui/locales/en.ts', 'docs/next/x.json', 'AGENTS.md', 'Cargo.lock'])], ['engine', 'ui', 'Cargo.lock']);
  assert.ok(workflow(['.github/workflows/ci.yml']) && !workflow(['.github/pull_request_template.md']));
});

test('a change is checked on top of the deepest queued batch that touches other parts, is not red and has room', () => {
  const q = (commit, files, depth, red = false) => ({ commit, files, depth, red });
  const queued = [q('a', ['engine/src/x.rs'], 1), q('b', ['engine/src/x.rs', 'ui/A.tsx'], 2), q('c', ['testkit/src/y.rs'], 1, true)];
  assert.equal(stackOn(queued, ['scripts/land.mjs'])?.commit, 'b', 'the deepest that fits');
  assert.equal(stackOn(queued, ['ui/B.tsx'])?.commit, 'a', 'one that shares a part is passed over');
  assert.equal(stackOn(queued, ['engine/src/z.rs', 'ui/B.tsx']), null, 'nothing fits: alone on the base');
  assert.equal(stackOn([q('d', ['ui/A.tsx'], 1, true)], ['engine/src/z.rs']), null, 'a red one is never built on');
  assert.equal(stackOn([q('e', ['ui/A.tsx'], BATCH)], ['engine/src/z.rs']), null, 'a full batch takes no more');
  assert.equal(stackOn([q('f', ['ui/A.tsx'], 1)], ['.github/workflows/ci.yml']), null, 'a workflow change lands from a PC, alone');
  assert.equal(stackOn([q('g', ['.github/workflows/ci.yml'], 1)], ['ui/B.tsx']), null, 'and nothing is built on one');
  assert.equal(stackOn([q('h', ['docs/a.md', 'ui/locales/en.ts'], 1)], ['ui/locales/en.ts', 'engine/src/a.rs'])?.commit, 'h', 'shared words do not keep two apart');
});

test('an already ineligible deeper batch is skipped while an active shallower batch remains usable', () => {
  const queued = [
    { commit: 'valid-shallow', files: ['engine/src/x.rs'], depth: 1, red: false, eligible: true },
    { commit: 'closed-deep', files: ['engine/src/x.rs', 'ui/A.tsx'], depth: 2, red: false, eligible: false },
  ];
  assert.equal(stackOn(queued, ['scripts/land.mjs'])?.commit, 'valid-shallow');
  assert.equal(stackOn([{ ...queued[0], eligible: false }], ['scripts/land.mjs']), null);
});

test('a stacked batch is eligible only while every parent is open, unchanged, queued and green', () => {
  const batch = [{ commit: 'a', pr: '1', oid: 'head-a' }, { commit: 'b', pr: '2', oid: 'head-b' }];
  const open = { state: 'OPEN', isDraft: false, baseRefName: 'phase-1-base', isCrossRepository: false };
  const views = { 1: { ...open, headRefOid: 'head-a' }, 2: { ...open, headRefOid: 'head-b' } };
  assert.equal(eligibleStack(batch, views, { 1: 'a', 2: 'b' }, new Set()), true);
  assert.equal(eligibleStack(batch, { ...views, 2: { ...views[2], state: 'CLOSED' } }, { 1: 'a', 2: 'b' }, new Set()), false);
  assert.equal(eligibleStack(batch, { ...views, 2: { ...views[2], headRefOid: 'pushed-again' } }, { 1: 'a', 2: 'b' }, new Set()), false);
  assert.equal(eligibleStack(batch, views, { 1: 'a', 2: 'requeued' }, new Set()), false);
  assert.equal(eligibleStack(batch, views, { 1: 'a', 2: 'b' }, new Set(['a'])), false);
  assert.equal(eligibleStack(batch, { ...views, 2: null }, { 1: 'a', 2: 'b' }, new Set()), false, 'unknown remote state fails closed');
  assert.equal(eligibleStack(batch, { ...views, 2: {} }, { 1: 'a', 2: 'b' }, new Set()), false, 'incomplete remote state fails closed');
});

test('a batch is read oldest first from the log, and a commit that is not a landed pull request stops it', () => {
  const log = 'aaa\0fix: one (#7)\0h7\n\x1ebbb\0feat: two (#9)\0h9\n\x1e';
  assert.deepEqual(stacked(log), [{ commit: 'aaa', pr: '7', oid: 'h7' }, { commit: 'bbb', pr: '9', oid: 'h9' }]);
  assert.equal(stacked('ccc\0chore: by hand\0\n\x1e'), null);
  assert.deepEqual(stacked(''), []);
});

test('a failed local proof names the suite and its exit status even when its output was empty', async () => {
  const { localProofFailure } = await import('./land.mjs');
  assert.equal(localProofFailure('rust', { status: 1, signal: null }), 'Local rust proof failed (exit 1). Nothing was queued.');
  assert.equal(localProofFailure('ui', { status: null, signal: 'SIGTERM' }), 'Local ui proof failed (signal SIGTERM). Nothing was queued.');
  assert.equal(localProofFailure('screen', { code: 'ENOENT' }), 'Local screen proof failed (start error ENOENT). Nothing was queued.');
  assert.equal(localProofFailure('rust', {}), 'Local rust proof failed (no exit status). Nothing was queued.');
});

test('the sweep picks green queue runs whose landing was dropped, oldest first, and skips the rest', () => {
  const r = (databaseId, headBranch, headSha, createdAt, conclusion = 'cancelled', status = 'completed') => ({ databaseId, headBranch, headSha, createdAt, conclusion, status });
  const runs = [
    r(5, 'queue/pr-5', 'e', '2026-10-30T10:05:00Z'),
    r(1, 'queue/pr-1', 'a', '2026-10-30T10:01:00Z'),
    r(2, 'queue/pr-2', 'b', '2026-10-30T10:02:00Z', 'failure'), // red: its author fixes it
    r(3, 'queue/pr-3', 'old', '2026-10-30T10:03:00Z'), // pushed again since: the new commit has its own run
    r(4, 'queue/pr-4', 'd', '2026-10-30T10:04:00Z'), // landed: its branch is gone
    r(7, 'queue/pr-7', 'g', '2026-10-30T10:07:00Z'),
    r(8, 'queue/pr-7', 'g', '2026-10-30T10:08:00Z', null, 'in_progress'), // a newer run of the same branch is on it
    r(9, 'phase-1-base', 'h', '2026-10-30T10:09:00Z'),
  ];
  const heads = new Map([['queue/pr-1', 'a'], ['queue/pr-2', 'b'], ['queue/pr-3', 'new'], ['queue/pr-5', 'e'], ['queue/pr-7', 'g']]);
  assert.deepEqual(dropped(runs, heads).map((x) => x.databaseId), [1, 5]);
  assert.deepEqual(dropped([], heads), []);
});

test('only a landing cancelled after a green gate is dropped', () => {
  assert.equal(landCancelled([{ name: 'gate', conclusion: 'success' }, { name: 'land', conclusion: 'cancelled' }]), true);
  assert.equal(landCancelled([{ name: 'gate', conclusion: 'cancelled' }, { name: 'land', conclusion: 'skipped' }]), false);
  assert.equal(landCancelled([{ name: 'gate', conclusion: 'success' }, { name: 'land', conclusion: 'failure' }]), false);
});

test('a green batch lands on a base moved by words only, and anything else sends it back to be checked again', () => {
  const task = 'docs/tasks/030-a-task.md';
  assert.equal(wordsOnly([task, 'docs/dev/workflow.md'], [task, 'docs/dev/workflow.md'], isWords), true);
  // Code moved the base, or code differs from the tree checked: checked again.
  assert.equal(wordsOnly([task, 'engine/src/engine.rs'], [task], isWords), false);
  assert.equal(wordsOnly([task], [task, 'ui/App.tsx'], isWords), false);
  // A page a test reads is code here, and a locale line moved by the numbering is too.
  assert.equal(wordsOnly(['docs/product-map.md'], ['docs/product-map.md'], isWords), false);
  assert.equal(wordsOnly([task], [task, 'ui/locales/en.ts'], isWords), false);
  // Numbers taken on the base meanwhile could collide with the batch's own.
  assert.equal(wordsOnly(['docs/decisions.md'], ['docs/decisions.md'], isWords), false);
  assert.equal(wordsOnly(['docs/next/other-lane.json'], [], isWords), false);
  assert.equal(wordsOnly([], [], isWords), false);
});

test('a green batch lands as checked when the base moved in independent files', () => {
  const check = (moved, changed, differs) => landsAsChecked(moved, changed, differs, isWords);
  const a = 'engine/src/a.rs';
  const b = 'ui/App.tsx';
  const flow = '.github/workflows/ci.yml';
  assert.equal(check(['docs/tasks/a.md'], [b], ['docs/tasks/a.md']), 'words');
  assert.equal(check([a], [b], [a]), 'disjoint');
  assert.equal(check([a], [a], [a]), null);
  assert.equal(check([a], [b], [a, 'other.ts']), null);
  assert.equal(check([flow], [b], [flow]), null);
  assert.equal(check([a], [flow], [a]), null);
  for (const numbered of ['docs/decisions.md', 'docs/next/a.json', 'ui/locales/en.ts']) {
    assert.equal(check([numbered], ['ui/locales/fr.ts'], [numbered]), null);
    assert.equal(check([numbered], [b], [numbered]), 'disjoint');
  }
  assert.equal(check([], [b], []), null);
});

test('main accepts a full phase-1-base dispatch after its gate passes', () => {
  const ci = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
  const main = ci.split('\n  main:\n')[1].split('\n  land:\n')[0];
  assert.match(main, /github\.event_name == 'workflow_dispatch' && !inputs\.warm/);
  assert.match(main, /github\.ref == 'refs\/heads\/phase-1-base'/);
  assert.match(main, /needs\.gate\.result == 'success'/);
  assert.match(main, /gh workflow run auto-release\.yml --ref phase-1-base -f sha="\$GITHUB_SHA"/);
});

test('a train takes the oldest queued riders that still stand, never a workflow change or one red alone (R-next)', () => {
  const view = (oid) => ({ state: 'OPEN', isDraft: false, baseRefName: 'phase-1-base', isCrossRepository: false, headRefOid: oid });
  const rider = (pr, time, more = {}) => ({ pr, oid: `h${pr}`, files: ['ui/App.tsx'], time, red: false, view: view(`h${pr}`), ...more });
  const entries = [
    rider('9', 30),
    rider('4', 10),
    rider('5', 20, { files: ['.github/workflows/ci.yml'] }),
    rider('6', 15, { red: true }),
    rider('7', 12, { view: view('pushed-again') }),
    rider('8', 11, { view: { ...view('h8'), state: 'CLOSED' } }),
    rider('3', 10, { oid: '' }),
    rider('2', 10, { view: null }),
  ];
  assert.deepEqual(riders(entries).map((e) => e.pr), ['4', '9']);
  assert.deepEqual(riders(entries, ['9']).map((e) => e.pr), ['9']);
  const many = Array.from({ length: TRAIN + 3 }, (_, i) => rider(String(i + 10), i));
  assert.equal(riders(many).length, TRAIN);
  assert.equal(riders(many).at(-1).pr, String(TRAIN + 9), 'the newest ones wait for the next train');
});

test('a red train rides again with its first half, down to one rider who answers alone', () => {
  assert.deepEqual(half(['1', '2', '3', '4', '5']), ['1', '2', '3']);
  assert.deepEqual(half(['1', '2']), ['1']);
  assert.deepEqual(half(['1']), ['1']);
});

test('a train runs while a run of its commit has not ended, other than the job asking, or was just sent', () => {
  assert.equal(running([{ id: 1, status: 'in_progress' }], '2', 9999), true);
  assert.equal(running([{ id: 1, status: 'queued' }], '1', 9999), false, 'its own landing does not hold the next train');
  assert.equal(running([{ id: 1, status: 'completed' }], '', 30), false, 'a run that ended without landing holds nothing');
  assert.equal(running([], '', 30), true, 'GitHub may not list a run started seconds ago');
  assert.equal(running([], '', 900), false);
});

test('only a red report of the very queue commit keeps a pull request off trains', () => {
  const commit = 'abcdef1234567890';
  assert.equal(redAt([{ state: 'failure', description: 'Queued as abcdef123, but its checks failed.' }], commit), true);
  assert.equal(redAt([{ state: 'failure', description: 'Queued as 999999999, but its checks failed.' }], commit), false, 'a late report of an older queueing');
  assert.equal(redAt([{ state: 'pending', description: 'Queued as abcdef123' }, { state: 'failure', description: 'abcdef123' }], commit), false);
  assert.equal(redAt([], commit), false);
});
