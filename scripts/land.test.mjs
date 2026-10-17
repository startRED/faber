// node --test scripts: a pull request lands as one commit named after it, and the helper says why one cannot.

import assert from 'node:assert/strict';
import test from 'node:test';
import { conflicts, message, refusal } from './land.mjs';

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
