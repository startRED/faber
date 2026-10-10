import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';

const root = join(import.meta.dirname, '..');

// Rust can live outside the default folders; see AGENTS.md.
function cargo(): { bin: string; env: NodeJS.ProcessEnv } {
  const exe = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
  const path = process.env.PATH ?? process.env.Path ?? '';
  const dirs = [...path.split(delimiter), process.env.CARGO_HOME && join(process.env.CARGO_HOME, 'bin'), join(homedir(), '.cargo', 'bin')];
  const dir = dirs.find((d) => d && existsSync(join(d, exe)));
  if (!dir) return { bin: 'cargo', env: process.env };
  const env: NodeJS.ProcessEnv = { ...process.env, PATH: `${dir}${delimiter}${path}`, CARGO_HOME: process.env.CARGO_HOME ?? dirname(dir) };
  delete env.Path;
  const rustup = join(dirname(dirname(dir)), 'rustup');
  if (!env.RUSTUP_HOME && existsSync(rustup)) env.RUSTUP_HOME = rustup;
  return { bin: join(dir, exe), env };
}

test('shell connects the setup guide to the engine (commands, loopback source, hidden login)', () => {
  const { bin, env } = cargo();
  const run = spawnSync(bin, ['test', '-p', 'faber', '--', 'setup'], {
    cwd: root, env, shell: false, stdio: 'pipe', timeout: 1_800_000,
  });
  assert.equal(run.error, undefined, `cargo could not start: ${run.error}`);
  const stdout = (run.stdout?.toString() ?? '') + (run.stderr?.toString() ?? '');
  assert.equal(run.status, 0, `The setup acceptance proofs failed:\n${stdout}`);
  assert.match(stdout, /test setup::.*ok/, 'Expected setup tests to execute and pass');
});
