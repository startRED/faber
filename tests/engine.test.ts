import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createEngine, withHistory, type AppEvent } from '../src/engine/engine.ts';
import { resolveCli } from '../src/engine/resolve-cli.ts';
import { pickModel } from '../src/engine/router.ts';
import type { Message } from '../src/engine/store.ts';
import { fixture, replay } from './support/replay.ts';

const dataDir = () => mkdtempSync(join(tmpdir(), 'faber-engine-'));

function engineWith(dir: string, cassette: string, seen: { args: string[] }) {
  const events: AppEvent[] = [];
  let finished: (m: Message) => void = () => {};
  const done = new Promise<Message>((r) => (finished = r));
  const engine = createEngine(
    {
      dataDir: dir,
      clis: { claude: { command: 'claude', prefix: [] } },
      spawn: (_bin, args) => {
        seen.args = args;
        return replay(fixture('claude', cassette));
      },
    },
    (e) => {
      events.push(e);
      if (e.type === 'message' && e.message.status !== 'streaming') finished(e.message);
    },
  );
  return { engine, events, done };
}

test('a chat turn streams to the screen and is saved with model, reason, session and cost', async () => {
  const dir = dataDir();
  const seen = { args: [] as string[] };
  const { engine, events, done } = engineWith(dir, 'chat', seen);
  const id = engine.send({ area: 'chat', text: 'In one sentence: why is the sky blue?', choice: { vendor: 'claude', model: 'haiku' } });
  const reply = await done;
  engine.shutdown();
  assert.equal(reply.status, 'done');
  assert.match(reply.text, /Rayleigh/);
  assert.equal(reply.model, 'claude-haiku-5-5');
  assert.equal(reply.reason, 'chosen by you');
  assert.ok(seen.args.includes('--restricted'));
  assert.ok(events.some((e) => e.type === 'quota' && e.vendor === 'claude'));
  const saved = JSON.parse(readFileSync(join(dir, 'conversations', `${id}.json`), 'utf8'));
  assert.equal(saved.sessions.claude, 'c5b9834c-dfc7-4086-beed-2212be93b722');
  assert.equal(saved.title, 'In one sentence: why is the sky blue?');
  assert.ok(saved.messages[1].costUsd > 0);
});

test('after reopening the app, the conversation resumes its CLI session', async () => {
  const dir = dataDir();
  const first = engineWith(dir, 'chat', { args: [] });
  const id = first.engine.send({ area: 'chat', text: 'In one sentence: why is the sky blue?', choice: { vendor: 'claude', model: 'haiku' } });
  await first.done;
  first.engine.shutdown();

  // The resume recording continues session 3ee0bd99; point the saved conversation at it.
  const file = join(dir, 'conversations', `${id}.json`);
  const saved = JSON.parse(readFileSync(file, 'utf8'));
  saved.sessions.claude = '3ee0bd99-72d2-4c5b-8420-a9f85ee53981';
  (await import('node:fs')).writeFileSync(file, JSON.stringify(saved));

  const seen = { args: [] as string[] };
  const second = engineWith(dir, 'resume', seen);
  second.engine.send({
    conversationId: id,
    area: 'chat',
    text: 'What single word did you reply with last time? Answer with only that word.',
    choice: { vendor: 'claude', model: 'haiku' },
  });
  const reply = await second.done;
  second.engine.shutdown();
  assert.equal(seen.args[seen.args.indexOf('--resume') + 1], '3ee0bd99-72d2-4c5b-8420-a9f85ee53981');
  assert.equal(reply.text.trim(), 'pong');
  assert.equal(second.engine.open(id)?.messages.length, 4);
});

test('a model joining midway gets the earlier conversation as text', () => {
  const text = withHistory(
    [
      { id: '1', role: 'user', text: 'hi', at: 0 },
      { id: '2', role: 'assistant', text: 'hello', at: 0 },
    ],
    'and now?',
  );
  assert.match(text, /User: hi\n\nAssistant: hello\n\nNow the user says:\nand now\?$/);
});

test('Auto picks a connected vendor and says why; a missing one is refused', () => {
  assert.deepEqual(pickModel({ vendor: 'auto' }, ['codex']), { vendor: 'codex', reason: 'Auto: the only connected subscription' });
  assert.equal(pickModel({ vendor: 'auto' }, ['claude', 'codex'])?.vendor, 'claude');
  assert.equal(pickModel({ vendor: 'claude' }, ['codex']), undefined);
  assert.equal(pickModel({ vendor: 'auto' }, []), undefined);
});

test('CLI lookup follows npm .cmd shims on Windows to the real program', () => {
  const files: Record<string, string> = {
    'C:\\npm\\claude.cmd': '@ECHO off\r\n"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe"   %*\r\n',
    'C:\\npm\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe': '',
    'C:\\npm\\gemini.cmd': '@IF EXIST "%~dp0\\node.exe" (\r\n  "%~dp0\\node.exe"  "%~dp0\\node_modules\\@google\\gemini-cli\\dist\\index.js" %*\r\n)',
    'C:\\node\\node.exe': '',
    'C:\\codex\\codex.exe': '',
  };
  const env = { PATH: 'C:\\npm;C:\\codex;C:\\node' };
  const has = (p: string) => p in files;
  const read = (p: string) => files[p]!;
  assert.deepEqual(resolveCli('claude', env, 'win32', has, read), {
    command: 'C:\\npm\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe',
    prefix: [],
  });
  assert.deepEqual(resolveCli('codex', env, 'win32', has, read), { command: 'C:\\codex\\codex.exe', prefix: [] });
  assert.deepEqual(resolveCli('gemini', env, 'win32', has, read), {
    command: 'C:\\node\\node.exe',
    prefix: ['C:\\npm\\node_modules\\@google\\gemini-cli\\dist\\index.js'],
  });
  assert.equal(resolveCli('missing', env, 'win32', has, read), undefined);
  assert.deepEqual(resolveCli('claude', { PATH: '/usr/bin' }, 'linux', (p) => p === '/usr/bin/claude', read), {
    command: '/usr/bin/claude',
    prefix: [],
  });
});
