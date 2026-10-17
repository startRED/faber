// node --test scripts: a pull request lands as one commit named after it, and the helper says why one cannot.

import assert from 'node:assert/strict';
import test from 'node:test';
import { conflicts, insertions, message, refusal, settle, WORDS } from './land.mjs';

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
