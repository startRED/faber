import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createEngine, withHistory, type AppEvent } from '../src/engine/engine.ts';
import { resolveCli } from '../src/engine/resolve-cli.ts';
import { pickModel } from '../src/engine/router.ts';
import { openTerminal } from '../src/main/terminal.ts';
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

test('a chat turn streams to the screen and is saved with model, reason, session and cost', { timeout: 5000 }, async () => {
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

test('after reopening the app, the conversation resumes its CLI session', { timeout: 5000 }, async () => {
  const dir = dataDir();
  const first = engineWith(dir, 'chat', { args: [] });
  const id = first.engine.send({ area: 'chat', text: 'In one sentence: why is the sky blue?', choice: { vendor: 'claude', model: 'haiku' } });
  await first.done;
  first.engine.shutdown();

  // The resume recording continues session 3ee0bd99; point the saved conversation at it.
  const file = join(dir, 'conversations', `${id}.json`);
  const saved = JSON.parse(readFileSync(file, 'utf8'));
  saved.sessions.claude = '3ee0bd99-72d2-4c5b-8420-a9f85ee53981';
  writeFileSync(file, JSON.stringify(saved));

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

test('a new chat starts its CLI while the user types and keeps it for the first message', { timeout: 5000 }, async () => {
  let spawns = 0;
  const dir = dataDir();
  let finished: (m: Message) => void = () => {};
  const done = new Promise<Message>((r) => (finished = r));
  const engine = createEngine(
    { dataDir: dir, clis: { claude: { command: 'claude', prefix: [] } }, spawn: () => (spawns++, replay(fixture('claude', 'chat'))) },
    (e) => e.type === 'message' && e.message.status !== 'streaming' && finished(e.message),
  );
  const choice = { vendor: 'claude', model: 'haiku' } as const;
  engine.warm({ area: 'chat', choice });
  engine.warm({ area: 'chat', choice });
  assert.equal(spawns, 1);
  assert.deepEqual(engine.state().conversations, []);
  engine.send({ area: 'chat', text: 'In one sentence: why is the sky blue?', choice });
  assert.equal((await done).status, 'done');
  assert.equal(spawns, 1);
  assert.equal(engine.state().conversations.length, 1);
  engine.shutdown();
});

test('"always allow" answers the next request for the same tool without asking again', { timeout: 5000 }, async () => {
  // The recording denied the second Write; here the engine must allow it by itself.
  let answers = 0;
  const entries = fixture('claude', 'tools').map((e) =>
    e.msg.type === 'control_response' && answers++ === 1
      ? { ...e, msg: { ...e.msg, response: { ...e.msg.response, response: { behavior: 'allow' } } } }
      : e,
  );
  const asked: string[] = [];
  let finished: () => void = () => {};
  const done = new Promise<void>((r) => (finished = r));
  let id = '';
  const engine = createEngine({ dataDir: dataDir(), clis: { claude: { command: 'claude', prefix: [] } }, spawn: () => replay(entries) }, (e) => {
    if (e.type !== 'message') return;
    const p = e.message.permissions?.[0];
    if (p && !asked.includes(p.requestId)) {
      asked.push(p.requestId);
      setImmediate(() => engine.answer(id, p.requestId, true, true));
    }
    if (e.message.status !== 'streaming') finished();
  });
  id = engine.send({
    area: 'chat',
    text: "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.",
    choice: { vendor: 'claude' },
  });
  await done;
  engine.shutdown();
  assert.equal(asked.length, 1);
  assert.deepEqual(engine.open(id)?.allowAlways, ['Write']);
});

test('two permission requests at once wait in a queue and both get answered', { timeout: 5000 }, async () => {
  const shown: number[] = [];
  const answered = new Set<string>();
  let finished: (m: Message) => void = () => {};
  const done = new Promise<Message>((r) => (finished = r));
  let id = '';
  const engine = createEngine(
    { dataDir: dataDir(), clis: { claude: { command: 'claude', prefix: [] } }, spawn: () => replay(fixture('claude', 'parallel')) },
    (e) => {
      if (e.type !== 'message') return;
      const waiting = e.message.permissions ?? [];
      if (waiting.length) shown.push(waiting.length);
      const first = waiting[0];
      if (first && !answered.has(first.requestId)) {
        answered.add(first.requestId);
        setImmediate(() => engine.answer(id, first.requestId, true));
      }
      if (e.message.status !== 'streaming') finished(e.message);
    },
  );
  id = engine.send({
    area: 'chat',
    text: 'Search the web for the latest stable version of Node.js and, in parallel, for the latest version of Python. Run both searches at the same time, then answer in two lines.',
    choice: { vendor: 'claude' },
  });
  const reply = await done;
  engine.shutdown();
  assert.equal(answered.size, 2);
  assert.ok(shown.includes(2), 'both requests were waiting at the same time');
  assert.equal(reply.status, 'done');
  assert.equal(reply.permissions, undefined);
});

test('text blocks around a tool are kept apart', { timeout: 5000 }, async () => {
  let finished: (m: Message) => void = () => {};
  const done = new Promise<Message>((r) => (finished = r));
  let id = '';
  let asked = 0;
  const engine = createEngine(
    { dataDir: dataDir(), clis: { codex: { command: 'codex', prefix: [] } }, spawn: () => replay(fixture('codex', 'tools')) },
    (e) => {
      if (e.type !== 'message') return;
      const p = e.message.permissions?.[0];
      if (p && Number(p.requestId) === asked) setImmediate(() => engine.answer(id, p.requestId, asked++ === 0));
      if (e.message.status !== 'streaming') finished(e.message);
    },
  );
  id = engine.send({ area: 'project', projectId: engine.addProject(dataDir()).id, text: 'Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.', choice: { vendor: 'codex' } });
  const reply = await done;
  engine.shutdown();
  const [commentary, final] = reply.text.split('\n\n');
  assert.match(commentary!, /a\.txt/);
  assert.ok(final && final.length > 0, `expected two blocks in: ${reply.text}`);
});

test('editing a message branches into a new conversation and leaves the original as it was', { timeout: 5000 }, async () => {
  const dir = dataDir();
  const dones: Message[] = [];
  let next: () => void = () => {};
  const engine = createEngine({ dataDir: dir, clis: { claude: { command: 'claude', prefix: [] } }, spawn: () => replay(fixture('claude', 'chat')) }, (e) => {
    if (e.type === 'message' && e.message.status !== 'streaming') {
      dones.push(e.message);
      next();
    }
  });
  const wait = () => new Promise<void>((r) => (next = r));
  const text = 'In one sentence: why is the sky blue?';
  const choice = { vendor: 'claude', model: 'haiku' } as const;
  let done = wait();
  const id = engine.send({ area: 'chat', text, choice });
  await done;
  writeFileSync(join(dir, 'chat', id, 'notes.txt'), 'made earlier');
  const first = engine.open(id)!.messages[0]!;
  done = wait();
  const branchId = engine.branch({ conversationId: id, messageId: first.id, text, choice });
  await done;
  engine.shutdown();
  assert.notEqual(branchId, id);
  assert.equal(engine.open(id)!.messages.length, 2);
  const branch = engine.open(branchId)!;
  assert.deepEqual(branch.branchOf, { conversationId: id, messageId: first.id });
  assert.equal(branch.messages.length, 2);
  assert.equal(readFileSync(join(dir, 'chat', branchId, 'notes.txt'), 'utf8'), 'made earlier');
});

test('"Open in terminal" resumes the same CLI session in the conversation folder; shell characters are refused', { timeout: 5000 }, async () => {
  const dir = dataDir();
  const { engine, done } = engineWith(dir, 'chat', { args: [] });
  const id = engine.send({ area: 'chat', text: 'In one sentence: why is the sky blue?', choice: { vendor: 'claude', model: 'haiku' } });
  await done;
  engine.shutdown();
  assert.deepEqual(engine.terminal(id), {
    command: 'claude',
    args: ['--resume', 'c5b9834c-dfc7-4086-beed-2212be93b722'],
    cwd: join(dir, 'chat', id),
  });
  assert.equal(engine.terminal('missing'), undefined);
  assert.throws(() => openTerminal({ command: 'claude', args: ['--resume', 'x & del *'], cwd: dir }), /Refusing/);
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
  assert.deepEqual(pickModel({ vendor: 'auto' }, ['codex']), { vendor: 'codex', effort: 'medium', reason: 'Auto: the only connected subscription' });
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
