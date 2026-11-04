import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
assert.ok(existsSync(new URL('./cli-qualification.mjs', import.meta.url)), 'nightly candidate contract checker is not implemented');
const { checkHelp, checkSchema, checkRevertSchema, checkVersion, isolatedEnv, report } = await import('./cli-qualification.mjs');

test('required Claude launch options must be actual help tokens', () => {
  const help = '--input-format --output-format --verbose --include-partial-messages --permission-prompt-tool --safe-mode --restricted --strict-mcp-config --setting-sources --no-session-persistence';
  assert.deepEqual(checkHelp('claude', help), []);
  assert.deepEqual(checkHelp('claude', help.replace('--restricted', '--restricted-later')), ['--restricted']);
});

test('Codex launch and schema-generation options are checked separately', () => {
  assert.deepEqual(checkHelp('codex', '--disable --config app-server'), []);
  assert.deepEqual(checkHelp('codex', '--disable app-server'), ['--config']);
});

const methods = ['initialize', 'thread/start', 'thread/resume', 'turn/start', 'turn/interrupt'];
const schema = (names) => ({ oneOf: names.map((name) => ({ properties: { method: { type: 'string', enum: [name] } }, required: ['method'] })) });
test('schema requires real request method branches, never prose or unrelated enums', () => {
  assert.deepEqual(checkSchema(schema(methods)), []);
  assert.deepEqual(checkSchema({ description: methods.join(' '), enum: methods }), methods);
  assert.deepEqual(checkSchema(schema(methods.slice(0, -1))), ['turn/interrupt']);
  assert.deepEqual(checkSchema({ definitions: { unused: schema(methods) } }), methods);
});

test('rewind requires the revert method and both required string identifiers', () => {
  const requests = schema(['thread/revert']);
  const params = { type: 'object', required: ['threadId', 'beforeTurnId'], properties: { threadId: { type: 'string' }, beforeTurnId: { type: 'string' } } };
  assert.equal(checkRevertSchema(requests, params), true);
  assert.equal(checkRevertSchema(schema([]), params), false);
  assert.equal(checkRevertSchema(requests, { ...params, required: ['threadId'] }), false);
  assert.equal(checkRevertSchema(requests, { ...params, properties: { ...params.properties, beforeTurnId: { type: 'integer' } } }), false);
});

test('recorded Codex rewind retains the first turn and resumes with its context', () => {
  const entries = readFileSync(new URL('../tests/fixtures/codex/native-rewind.jsonl', import.meta.url), 'utf8')
    .trim().split(/\r?\n/).map((line) => JSON.parse(line));
  const requests = entries.filter((entry) => entry.dir === 'in').map((entry) => entry.msg);
  const turns = requests.filter((msg) => msg.method === 'turn/start');
  const response = (id) => entries.find((entry) => entry.dir === 'out' && entry.msg.id === id)?.msg.result;
  const revert = requests.find((msg) => msg.method === 'thread/revert');
  const list = requests.find((msg) => msg.method === 'thread/turns/list');
  const resume = requests.find((msg) => msg.method === 'thread/resume');
  assert.equal(revert.params.beforeTurnId, response(turns[1].id).turn.id);
  assert.equal(response(revert.id).thread.id, revert.params.threadId);
  assert.deepEqual(response(list.id).data.map((turn) => turn.id), [response(turns[0].id).turn.id]);
  assert.equal(resume.params.threadId, revert.params.threadId);
  assert.equal(response(resume.id).thread.id, revert.params.threadId);
  assert.equal(turns[2].params.input[0].text, 'What is the remembered code? Reply with the code only.');
  const finalTurn = response(turns[2].id).turn.id;
  assert.ok(entries.some((entry) => entry.dir === 'out'
    && entry.msg.method === 'item/completed'
    && entry.msg.params?.threadId === revert.params.threadId
    && entry.msg.params?.turnId === finalTurn
    && entry.msg.params?.item?.text === 'BLUE'));
});

test('version identity must agree with the installed exact package version', () => {
  assert.equal(checkVersion('claude', '2.1.310 (Claude Code)\n', '2.1.310'), true);
  assert.equal(checkVersion('codex', 'codex-cli 0.164.0\n', '0.164.0'), true);
  assert.equal(checkVersion('codex', 'codex-cli 0.164.1', '0.164.0'), false);
  assert.equal(checkVersion('claude', 'warning: 2.1.310', '2.1.310'), false);
});

test('probe environment retains only platform essentials, with isolated account folders', () => {
  const env = isolatedEnv('X:/disposable', { PATH: 'bin', SystemRoot: 'Windows', OPENAI_API_KEY: 'do not copy', ANTHROPIC_API_KEY: 'do not copy', CODEX_HOME: 'personal', HOME: 'personal', GITHUB_TOKEN: 'do not copy', NODE_OPTIONS: 'do not copy' });
  assert.equal(env.PATH, 'bin');
  assert.equal(env.HOME, 'X:/disposable');
  assert.equal(env.CODEX_HOME, 'X:/disposable');
  assert.equal(env.OPENAI_API_KEY, undefined);
  assert.equal(env.ANTHROPIC_API_KEY, undefined);
  assert.equal(env.GITHUB_TOKEN, undefined);
  assert.equal(env.NODE_OPTIONS, undefined);
});

test('passing candidate contracts still cannot qualify a version for installation', () => {
  const value = report({ provider: 'codex', version: '0.164.0', checks: [{ name: 'help', passed: true }], qualified: true });
  assert.equal(value.qualified, false);
  assert.equal(value.scope, 'unauthenticated-contract-only');
  assert.ok(value.missingEvidence.includes('authenticated-turn'));
});

test('startup accepts only the matching successful initialize response', async () => {
  const { checkInitialize } = await import('./cli-qualification.mjs');
  assert.equal(typeof checkInitialize, 'function', 'initialize response validation is missing');
  assert.equal(checkInitialize('{"id":1,"result":{"userAgent":"probe"}}'), true);
  for (const line of ['{', 'null', '{"id":2,"result":{"userAgent":"probe"}}',
    '{"id":1,"error":{"code":-1}}', '{"id":1,"result":{}}',
    '{"id":1,"result":{"userAgent":"probe"},"error":{}}']) {
    assert.equal(checkInitialize(line), false);
  }
});

test('only a passing provider startup removes its missing evidence', () => {
  for (const provider of ['codex', 'claude']) {
    for (const passed of [true, false]) {
      const value = report({ provider, checks: [{ name: 'protocol-startup', passed }] });
      assert.equal(value.missingEvidence.includes('protocol-startup'), !passed);
      assert.equal(value.qualified, false);
      assert.ok(value.missingEvidence.includes('process-tree-cleanup'));
    }
  }
});

test('Claude startup validates its correlated success envelope and closes without a user message', async (t) => {
  const { checkInitialize, startup } = await import('./cli-qualification.mjs');
  const success = { type: 'control_response', response: { subtype: 'success', request_id: 'faber-init-1', response: {} } };
  assert.equal(checkInitialize(JSON.stringify(success), 'claude'), true);
  for (const response of [null, [], { subtype: 'error', request_id: 'faber-init-1', response: {} },
    { subtype: 'success', request_id: 'other', response: {} },
    { subtype: 'success', request_id: 'faber-init-1', response: [] }]) {
    assert.equal(checkInitialize(JSON.stringify({ type: 'control_response', response }), 'claude'), false);
  }
  const child = new EventEmitter();
  for (const name of ['stdin', 'stdout', 'stderr']) child[name] = new PassThrough();
  child.kill = t.mock.fn();
  const sent = [];
  child.stdin.on('data', chunk => sent.push(chunk.toString()));
  const start = t.mock.fn(() => child);
  const pending = startup('unused', 'isolated', start, 'claude');
  child.stdout.write(JSON.stringify(success) + '\n');
  assert.equal(child.stdin.writableEnded, true);
  child.emit('close', 0);
  assert.deepEqual(await pending, { name: 'protocol-startup', passed: true });
  assert.deepEqual(sent.map(line => JSON.parse(line)), [
    { type: 'control_request', request_id: 'faber-init-1', request: { subtype: 'initialize' } },
  ]);
  const [, args, options] = start.mock.calls[0].arguments;
  assert.deepEqual(args, ['-p', '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose',
    '--system-prompt', '', '--tools', '', '--setting-sources', '', '--strict-mcp-config', '--mcp-config',
    '{"mcpServers":{}}', '--no-session-persistence']);
  assert.equal(options.env.CLAUDE_CONFIG_DIR, 'isolated');
  assert.equal(options.shell, false);
  assert.equal(options.windowsHide, true);
});

test('startup deadline completes even when inherited pipes never close', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const { startup } = await import('./cli-qualification.mjs');
  const child = new EventEmitter();
  for (const name of ['stdin', 'stdout', 'stderr']) child[name] = new PassThrough();
  child.kill = t.mock.fn();
  let result;
  const pending = startup('unused', 'unused', () => child).then((value) => { result = value; });
  t.mock.timers.tick(30_000);
  await Promise.resolve();
  assert.deepEqual(result, { name: 'protocol-startup', passed: false, reason: 'timeout' });
  assert.equal(child.kill.mock.calls.length, 1);
  for (const name of ['stdin', 'stdout', 'stderr']) assert.equal(child[name].destroyed, true);
  await pending;
});

test('startup caps combined output and rejects malformed or uncorrelated replies', async (t) => {
  const { startup } = await import('./cli-qualification.mjs');
  for (const [chunks, reason] of [
    [[['stderr', Buffer.alloc(600_000)], ['stdout', Buffer.alloc(600_000)]], 'output-limit'],
    [[['stdout', '{broken}\n']], 'malformed-json'],
    [[['stdout', '{"id":2,"result":{"userAgent":"probe"}}\n']], 'protocol-mismatch'],
  ]) {
    const child = new EventEmitter();
    for (const name of ['stdin', 'stdout', 'stderr']) child[name] = new PassThrough();
    child.kill = t.mock.fn();
    const pending = startup('unused', 'unused', () => child);
    for (const [stream, chunk] of chunks) child[stream].write(chunk);
    assert.deepEqual(await pending, { name: 'protocol-startup', passed: false, reason });
  }
});
