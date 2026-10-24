import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
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
