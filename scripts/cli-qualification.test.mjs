import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
assert.ok(existsSync(new URL('./cli-qualification.mjs', import.meta.url)), 'nightly candidate contract checker is not implemented');
const { checkHelp, checkSchema, checkVersion, isolatedEnv, report } = await import('./cli-qualification.mjs');

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

test('only a passing Codex startup removes its missing evidence', () => {
  for (const provider of ['codex', 'claude']) {
    for (const passed of [true, false]) {
      const value = report({ provider, checks: [{ name: 'protocol-startup', passed }] });
      assert.equal(value.missingEvidence.includes('protocol-startup'), !(provider === 'codex' && passed));
      assert.equal(value.qualified, false);
      assert.ok(value.missingEvidence.includes('process-tree-cleanup'));
    }
  }
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
