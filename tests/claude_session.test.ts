import { test } from 'node:test';
import assert from 'node:assert/strict';
import { claudeArgs } from '../src/engine/claude/args.ts';
import { startClaude } from '../src/engine/claude/session.ts';
import type { EngineEvent, Session } from '../src/engine/events.ts';
import { drive, fixture as cassette, textOf as text, type Entry } from './support/replay.ts';

const fixture = (name: string) => cassette('claude', name);
const run = (entries: Entry[], first: string, react: (e: EngineEvent, s: Session) => boolean) =>
  drive(entries, (spawn, on) => startClaude({ bin: 'claude', cwd: '.', launch: { area: 'chat' }, spawn }, on), first, react);
const recordedText = (entries: Entry[]) =>
  entries
    .filter((e) => e.msg.type === 'assistant')
    .flatMap((e) => e.msg.message.content)
    .filter((c: any) => c.type === 'text')
    .map((c: any) => c.text)
    .join('');

async function checkChat(entries: Entry[]) {
  const { events, mismatches } = await run(entries, 'In one sentence: why is the sky blue?', (e) => e.type === 'done');
  assert.deepEqual(mismatches, []);
  assert.deepEqual(events[0], { type: 'started', sessionId: 'c5b9834c-dfc7-4086-beed-2212be93b722', model: 'claude-haiku-5-5' });
  assert.equal(text(events), recordedText(entries));
  assert.ok(events.some((e) => e.type === 'quota' && e.window === 'five_hour' && e.used === 0.07));
  const done = events.at(-1);
  assert.ok(done?.type === 'done' && done.ok && (done.costUsd ?? 0) > 0 && done.outputTokens === 73);
}

test('chat: streams text, model, quota and cost from a recorded session', () => checkChat(fixture('chat')));

test('a renamed field in a recorded session turns the check red', async () => {
  const entries = fixture('chat').map((e) =>
    e.msg.subtype === 'init' ? { ...e, msg: { ...e.msg, sessionId: e.msg.session_id, session_id: undefined } } : e,
  );
  await assert.rejects(checkChat(entries));
});

test('permissions: the first Write is allowed and the second denied', async () => {
  const asks: string[] = [];
  const { events, mismatches } = await run(
    fixture('tools'),
    "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.",
    (e, s) => {
      if (e.type === 'permission') {
        asks.push(e.tool);
        s.answer(e.requestId, asks.length === 1);
      }
      return e.type === 'done';
    },
  );
  assert.deepEqual(mismatches, []);
  assert.deepEqual(asks, ['Write', 'Write']);
  const results = events.filter((e) => e.type === 'tool-result');
  assert.deepEqual(results.map((r) => r.type === 'tool-result' && r.ok), [true, false]);
});

test('interrupt stops the turn and the same session takes the next one', async () => {
  let deltas = 0;
  let dones = 0;
  const { events, mismatches } = await run(fixture('interrupt'), 'Write a 600-word essay about the history of bread.', (e, s) => {
    if (e.type === 'text' && ++deltas === 5) s.interrupt();
    if (e.type === 'done' && ++dones === 1) s.send({ text: 'Reply with just the word: pong' });
    return dones === 2;
  });
  assert.deepEqual(mismatches, []);
  const [first, second] = events.filter((e) => e.type === 'done');
  assert.ok(first?.type === 'done' && !first.ok);
  assert.ok(second?.type === 'done' && second.ok);
  assert.match(text(events.slice(events.indexOf(first!))), /pong/);
});

test('resume: a new process continues the same session', async () => {
  const { events, mismatches } = await run(
    fixture('resume'),
    'What single word did you reply with last time? Answer with only that word.',
    (e) => e.type === 'done',
  );
  assert.deepEqual(mismatches, []);
  assert.deepEqual(events[0], { type: 'started', sessionId: '3ee0bd99-72d2-4c5b-8420-a9f85ee53981', model: 'claude-haiku-5-5' });
  assert.equal(text(events).trim(), 'pong');
});

test('chat can never touch files; projects keep only project settings', () => {
  const chat = claudeArgs({ area: 'chat', resume: 'abc', model: 'haiku' });
  assert.ok(chat.includes('--restricted'));
  assert.equal(chat[chat.indexOf('--tools') + 1], 'WebSearch,WebFetch');
  assert.equal(chat[chat.indexOf('--resume') + 1], 'abc');
  assert.equal(chat[chat.indexOf('--model') + 1], 'haiku');
  const project = claudeArgs({ area: 'project' });
  assert.ok(!project.includes('--restricted'));
  assert.equal(project[project.indexOf('--setting-sources') + 1], 'project,local');
});
