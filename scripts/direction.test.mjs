// node --test scripts: the direction check asks the right question and writes proposals that never join the queue.

import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { buildPrompt, MARK, PROPOSED, pages, parseAnswer, SCHEMA, slug, tasks, write } from './direction.mjs';
import { parseName } from './tasks.mjs';

const answer = {
  goals_without_feature: [{ goal: 'Phone remote', source: 'docs/product/areas.md#Phone', why: 'Nothing reaches a phone.' }],
  drifting_features: [],
  tasks_off_goal: [{ task: 'docs/tasks/009-motion-leftovers.md', why: 'Done by R230.' }],
  proposals: [
    { name: '../../evil', title: 'Phone remote', brief: 'Build it. Done when a phone can follow a run.' },
    { name: 'phone-remote', title: 'Phone remote, part two', brief: 'More.' },
  ],
  could_not_judge: ['Whether "enterprise level" is met: no measure is written down.'],
};

function repo() {
  const root = mkdtempSync(join(tmpdir(), 'direction-test-'));
  mkdirSync(join(root, 'docs/product'), { recursive: true });
  mkdirSync(join(root, 'docs/tasks'), { recursive: true });
  writeFileSync(join(root, 'docs/product/models.md'), '# Models\n');
  writeFileSync(join(root, 'docs/product/README.md'), '# Product\n');
  writeFileSync(join(root, 'docs/tasks/README.md'), '# Task queue\n');
  writeFileSync(join(root, 'docs/tasks/020-b.md'), '# Second\n\nBrief.\n');
  writeFileSync(join(root, 'docs/tasks/010-a.md'), '# First\n\nBrief.\n');
  return root;
}

test('it reads the product pages, README first, then the map and the decisions, and the queue in order', () => {
  const root = repo();
  try {
    assert.deepEqual(pages(root), ['docs/product/README.md', 'docs/product/models.md', 'docs/product-map.md', 'docs/decisions.md']);
    assert.deepEqual(tasks(root), [
      { file: 'docs/tasks/010-a.md', title: 'First' },
      { file: 'docs/tasks/020-b.md', title: 'Second' },
    ]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('the question names every page and task, the three questions, and asks what it could not judge', () => {
  const prompt = buildPrompt(['docs/product/README.md', 'docs/decisions.md'], [{ file: 'docs/tasks/010-a.md', title: 'First' }], '2026-10-19');
  for (const part of ['docs/product/README.md', 'docs/decisions.md', 'docs/tasks/010-a.md: First', 'no feature yet', 'drift', 'no longer serve', 'could_not_judge', 'Do not edit']) {
    assert.ok(prompt.includes(part), part);
  }
  assert.ok(buildPrompt([], [], '2026-10-19').includes('the queue is empty'));
});

test('the answer schema is strict, and a missing list reads as empty', () => {
  assert.equal(SCHEMA.additionalProperties, false);
  assert.deepEqual(SCHEMA.required, Object.keys(SCHEMA.properties));
  assert.deepEqual(parseAnswer('```json\n{"proposals": []}\n```').goals_without_feature, []);
  assert.throws(() => parseAnswer('{"proposals": "x"}'), /not a list/);
});

test('a proposal name can never leave the folder', () => {
  assert.equal(slug('../../evil'), 'evil');
  assert.equal(slug('Phone Remote!'), 'phone-remote');
  assert.equal(slug('...'), 'proposal');
});

test('proposals are marked drafts outside the queue; a later run replaces its own and keeps the person\'s', () => {
  const root = repo();
  try {
    const dir = join(root, PROPOSED);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'mine.md'), '# My own draft\n');
    writeFileSync(join(dir, 'old.md'), `# Old\n\n${MARK} of 2026-10-01.\n`);
    const report = write(answer, '2026-10-19', root);
    assert.deepEqual(readdirSync(dir).sort(), ['REPORT.md', 'evil.md', 'mine.md', 'phone-remote.md']);
    const evil = readFileSync(join(dir, 'evil.md'), 'utf8');
    assert.ok(evil.startsWith('# Phone remote\n') && evil.includes(MARK) && evil.includes('not in the queue'));
    // The queue reads only <order>-<name>.md directly in docs/tasks, so nothing proposed is a task.
    assert.equal(parseName('proposed'), null);
    assert.ok(!existsSync(join(root, 'docs/tasks/evil.md')));
    for (const part of ['Phone remote: Nothing reaches a phone.', 'Done by R230.', `${PROPOSED}/evil.md`, 'no measure is written down', 'Nothing found.', 'nothing was queued']) {
      assert.ok(report.includes(part), part);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
