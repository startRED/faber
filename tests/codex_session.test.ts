import { test } from 'node:test';
import assert from 'node:assert/strict';
import { codexArgs, codexThread } from '../src/engine/codex/launch.ts';
import { parseCodexNotification } from '../src/engine/codex/parse.ts';
import { startCodex } from '../src/engine/codex/session.ts';
import type { EngineEvent, Session } from '../src/engine/events.ts';
import { drive, fixture as cassette, textOf as text, type Entry } from './support/replay.ts';

const fixture = (name: string) => cassette('codex', name);
const run = (entries: Entry[], first: string, react: (e: EngineEvent, s: Session) => boolean, resume?: string) =>
  drive(entries, (spawn, on) => startCodex({ bin: 'codex', cwd: '.', launch: { area: 'chat', resume }, spawn }, on), first, react);

const recordedAnswers = (entries: Entry[]) =>
  entries
    .filter((e) => e.msg.method === 'item/completed' && e.msg.params.item.type === 'agentMessage')
    .map((e) => e.msg.params.item.text);

async function checkChat(entries: Entry[]) {
  let dones = 0;
  const { events, mismatches } = await run(entries, 'In one sentence: why is the sky blue?', (e, s) => {
    if (e.type === 'done' && ++dones === 1) s.send({ text: 'Reply with just the word: pong' });
    return dones === 2;
  });
  assert.deepEqual(mismatches, []);
  const started = events[0];
  assert.ok(started?.type === 'started' && started.sessionId === '01a12513-0e42-7b63-9a21-c8698f522c12' && started.model === 'gpt-6-luna');
  assert.equal(text(events), recordedAnswers(entries).join(''));
  assert.ok(events.some((e) => e.type === 'quota' && e.window === 'seven_day' && e.used === 0.47));
  const done = events.filter((e) => e.type === 'done');
  assert.ok(done.every((d) => d.type === 'done' && d.ok && d.inputTokens > 0 && d.outputTokens > 0));
}

test('codex chat: two turns with text, model, quota and tokens', () => checkChat(fixture('chat')));

test('codex: a renamed field in a recorded session turns the check red', async () => {
  const entries = fixture('chat').map((e) =>
    e.msg.method === 'item/agentMessage/delta' ? { ...e, msg: { ...e.msg, params: { ...e.msg.params, delta: undefined, text: e.msg.params.delta } } } : e,
  );
  await assert.rejects(checkChat(entries));
});

test('codex permissions: the first command is allowed and the second declined', async () => {
  const asks: string[] = [];
  const { events, mismatches } = await run(
    fixture('tools'),
    'Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.',
    (e, s) => {
      if (e.type === 'permission') {
        asks.push(e.tool);
        s.answer(e.requestId, asks.length === 1);
      }
      return e.type === 'done';
    },
  );
  assert.deepEqual(mismatches, []);
  assert.deepEqual(asks, ['command', 'command']);
  const results = events.filter((e) => e.type === 'tool-result');
  assert.deepEqual(results.map((r) => r.type === 'tool-result' && r.ok), [true, false]);
});

test('codex interrupt stops the turn and the thread takes the next one', async () => {
  let deltas = 0;
  let dones = 0;
  const { events, mismatches } = await run(fixture('interrupt'), 'Write a 600-word essay about the history of bread.', (e, s) => {
    if (e.type === 'text' && ++deltas === 5) s.interrupt();
    if (e.type === 'done' && ++dones === 1) s.send({ text: 'Reply with just the word: pong' });
    return dones === 2;
  });
  assert.deepEqual(mismatches, []);
  const [first, second] = events.filter((e) => e.type === 'done');
  assert.ok(first?.type === 'done' && !first.ok && first.reason === 'interrupted');
  assert.ok(second?.type === 'done' && second.ok);
});

test('codex resume: a new process continues the same thread', async () => {
  const id = '01a1251a-252b-7031-bcb9-dd0b0adfc7ce';
  const { events, mismatches } = await run(
    fixture('resume'),
    'What single word did you reply with last time? Answer with only that word.',
    (e) => e.type === 'done',
    id,
  );
  assert.deepEqual(mismatches, []);
  assert.ok(events[0]?.type === 'started' && events[0].sessionId === id);
  assert.equal(text(events).trim(), 'pong');
});

test('codex: a message that arrives whole, without deltas, is still shown once', () => {
  const state = { input: 0, output: 0, streamed: new Set<string>() };
  const done = (id: string) => ({ method: 'item/completed', params: { item: { type: 'agentMessage', id, text: 'hi' } } });
  assert.deepEqual(parseCodexNotification(done('a'), state), [{ type: 'text', delta: 'hi' }]);
  parseCodexNotification({ method: 'item/agentMessage/delta', params: { itemId: 'b', delta: 'hi' } }, state);
  assert.deepEqual(parseCodexNotification(done('b'), state), []);
});

test('codex chat cannot run commands; projects can, behind approval', () => {
  const chat = codexArgs('chat', 'linux');
  assert.ok(chat.join(' ').includes('--disable shell_tool'));
  assert.ok(!chat.join(' ').includes('windows.sandbox'));
  assert.ok(codexArgs('project', 'win32').includes('windows.sandbox="unelevated"'));
  assert.ok(!codexArgs('project', 'win32').includes('shell_tool'));
  assert.deepEqual(
    { sandbox: codexThread('chat').sandbox, approval: codexThread('chat').approvalPolicy },
    { sandbox: 'read-only', approval: 'never' },
  );
  assert.equal(codexThread('project', 'gpt-6-luna').approvalPolicy, 'on-request');
});
