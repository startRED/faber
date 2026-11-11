import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { FIXED, load, problems, report, summarize } from './real-use.mjs';

const NOW = Date.UTC(2026, 10, 20);
const reply = (reason, tools = []) => ({ role: 'assistant', reason, at: NOW, tools });
const conversation = (reason, more = {}) => ({
  id: 'c', area: 'project', choice: { vendor: 'auto' }, updatedAt: NOW, messages: [{ role: 'user', text: 'a secret request' }, reply(reason)], ...more,
});

test('counts Auto starts nobody judged, apart for projects and for conversations a lead started', () => {
  const sum = summarize([
    conversation('reason.route.code.hard'),
    conversation('reason.autoImmediate'),
    conversation('reason.autoDefault', { startedBy: 'lead' }),
    conversation('reason.autoImmediate', { area: 'chat' }),
    // A hand-picked model and a conversation older than the span are not Auto starts of this span.
    conversation('reason.chosen', { choice: { vendor: 'claude' } }),
    conversation('reason.autoImmediate', { updatedAt: NOW - 30 * 86_400_000 }),
  ], NOW - 7 * 86_400_000);
  assert.deepEqual(sum.starts, { judged: 1, unjudged: 3, project: 3, projectUnjudged: 2, byLead: 1, byLeadUnjudged: 1 });
  assert.equal(sum.conversations, 5);
});

test('prints only what Faber itself said, with ids masked, and other tools by name', () => {
  const tools = [
    { name: 'Conversation', ok: false, output: 'No conversation was started: that step waits for a step that is not done.\nmore' },
    { name: 'Conversation', ok: false, output: 'Nothing to read: no conversation with id 0123456789abcdef is in this project.' },
    { name: 'Bash', ok: false, output: 'cat: the password is hunter2' },
    { name: 'Read', ok: true, output: 'fine' },
  ];
  const sum = summarize([conversation('reason.route.code.hard', { messages: [reply('reason.route.code.hard', tools)] })]);
  assert.equal(sum.steps, 4);
  assert.equal(sum.failed, 3);
  const text = report(sum, 7).join('\n');
  assert.match(text, /that step waits for a step that is not done/);
  assert.match(text, /no conversation with id <id> is in this project/);
  assert.match(text, /1 {2}Bash/);
  assert.doesNotMatch(text, /hunter2|secret request|0123456789abcdef/);
});

test('the check fails on unjudged project starts over the limit and on a fixed refusal that came back', () => {
  const clean = summarize([conversation('reason.route.code.hard'), conversation('reason.autoImmediate')]);
  assert.deepEqual(problems(clean), [], 'two starts are too few to judge');
  const many = summarize([1, 2, 3, 4].map((n) => conversation(n === 1 ? 'reason.route.code.easy' : 'reason.autoImmediate')));
  assert.match(problems(many)[0], /3 of 4 Auto starts in projects were never judged/);
  const refused = (at) => [{ name: 'Conversation', ok: false, ended: at, output: `No conversation was started: ${FIXED[0].text}; choose an AI.` }];
  const before = summarize([conversation('x', { messages: [reply('reason.chosen', refused(FIXED[0].since - 1))] })]);
  assert.deepEqual(problems(before), [], 'a refusal from before the fix is history');
  const after = summarize([conversation('x', { messages: [reply('reason.chosen', refused(FIXED[0].since + 1))] })]);
  assert.match(problems(after)[0], /a refusal that was fixed came back/);
});

test('reads a data folder and skips a file that does not parse', () => {
  const data = mkdtempSync(join(tmpdir(), 'faber-real-use-'));
  try {
    mkdirSync(join(data, 'conversations'));
    writeFileSync(join(data, 'conversations', 'a.json'), JSON.stringify(conversation('reason.autoImmediate')));
    writeFileSync(join(data, 'conversations', 'b.json'), '{ cut off');
    writeFileSync(join(data, 'conversations', 'notes.txt'), 'not a conversation');
    assert.equal(load(data).length, 1);
  } finally {
    rmSync(data, { recursive: true, force: true });
  }
});
