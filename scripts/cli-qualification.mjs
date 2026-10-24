// Candidate contract probes only. These receipts never authorize installation or a live manifest entry.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { isAbsolute, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const OPTIONS = {
  claude: ['--input-format', '--output-format', '--verbose', '--include-partial-messages', '--permission-prompt-tool', '--safe-mode', '--restricted', '--strict-mcp-config', '--setting-sources', '--no-session-persistence'],
  codex: ['--disable', '--config', 'app-server'],
};
const METHODS = ['initialize', 'thread/start', 'thread/resume', 'turn/start', 'turn/interrupt'];
const VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

export function checkHelp(provider, text) {
  if (!Object.hasOwn(OPTIONS, provider)) throw new Error('Unknown provider');
  const tokens = new Set(text.split(/[\s,|=<>[\]()]+/));
  return OPTIONS[provider].filter((option) => !tokens.has(option));
}

export function checkSchema(schema) {
  const methods = new Set((schema?.oneOf ?? []).flatMap((branch) =>
    branch.required?.includes('method') && branch.properties?.method?.type === 'string'
      ? branch.properties.method.enum ?? [] : []));
  return METHODS.filter((method) => !methods.has(method));
}

export function checkVersion(provider, text, version) {
  return VERSION.test(version) && text.trim() === (provider === 'claude' ? `${version} (Claude Code)` : `codex-cli ${version}`);
}

export function isolatedEnv(folder, source = process.env) {
  const env = {};
  for (const key of ['PATH', 'Path', 'SystemRoot', 'SYSTEMROOT', 'WINDIR', 'COMSPEC', 'PATHEXT']) {
    if (source[key]) env[key] = source[key];
  }
  for (const key of ['HOME', 'USERPROFILE', 'APPDATA', 'LOCALAPPDATA', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'CODEX_HOME', 'CLAUDE_CONFIG_DIR', 'TMP', 'TEMP', 'TMPDIR']) env[key] = folder;
  return { ...env, CI: '1', NO_COLOR: '1', DISABLE_AUTOUPDATER: '1', CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1' };
}

export function report(identity) {
  return { ...identity, scope: 'unauthenticated-contract-only', qualified: false,
    missingEvidence: ['protocol-startup', 'authenticated-turn', 'cancellation', 'process-tree-cleanup'] };
}

export function probe(provider, binary, version, destination) {
  if (!Object.hasOwn(OPTIONS, provider) || !VERSION.test(version) || !isAbsolute(binary) || !isAbsolute(destination)) {
    throw new Error('Expected provider, absolute native binary, exact package version and absolute report path');
  }
  const folder = mkdtempSync(join(tmpdir(), 'faber-cli-contract-'));
  const checks = [];
  const identity = { provider, version, platform: process.platform, arch: process.arch, checkedAt: new Date().toISOString() };
  try {
    if (statSync(binary).size > 512 * 1024 * 1024) throw new Error('binary-size');
    identity.binarySha256 = createHash('sha256').update(readFileSync(binary)).digest('hex');
    const run = (name, args, validate) => {
      const result = spawnSync(binary, args, { cwd: folder, env: isolatedEnv(folder), encoding: 'utf8',
        shell: false, windowsHide: true, timeout: 30_000, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
      const passed = !result.error && result.status === 0 && validate(result.stdout);
      checks.push({ name, passed: Boolean(passed), ...(passed ? {} : { reason: result.error?.code ?? (result.status !== 0 ? 'command-failed' : 'contract-mismatch') }) });
      return passed;
    };
    run('version', ['--version'], (text) => checkVersion(provider, text, version));
    run('launch-options', ['--help'], (text) => checkHelp(provider, text).length === 0);
    if (provider === 'codex') {
      const schemaFolder = join(folder, 'schema');
      run('request-schema', ['app-server', 'generate-json-schema', '--out', schemaFolder], () => {
        const path = join(schemaFolder, 'ClientRequest.json');
        if (statSync(path).size > 8 * 1024 * 1024) return false;
        return checkSchema(JSON.parse(readFileSync(path, 'utf8'))).length === 0;
      });
    }
  } catch {
    checks.push({ name: 'probe', passed: false, reason: 'probe-failed' });
  }
  const result = report({ ...identity, checks });
  writeFileSync(destination, `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [provider, binary, version, destination, ...extra] = process.argv.slice(2);
  if (extra.length) throw new Error('Unexpected arguments');
  const result = probe(provider, binary, version, destination);
  process.stdout.write(`${provider}: ${result.checks.filter((check) => check.passed).length}/${result.checks.length} contract checks passed; full qualification missing\n`);
  if (result.checks.some((check) => !check.passed)) process.exitCode = 1;
}
