import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export function plan(env) {
  const triple = env.TAURI_ENV_TARGET_TRIPLE || '';
  if (triple && !/^[a-z0-9_]+(-[a-z0-9_.]+){1,4}$/.test(triple)) throw new Error(`Unexpected target: ${triple}`);
  const target = path.join(root, 'target', 'cli-bin');
  return {
    args: [path.join(root, 'scripts', 'cargo.mjs'), 'build', '--release', '-p', 'faber-cli', ...(triple ? ['--target', triple] : [])],
    target,
    built: path.join(target, ...(triple ? [triple] : []), 'release', 'faber-cli.exe'),
    shipped: path.join(root, 'src-tauri', 'binaries', 'faber.exe'),
  };
}

function main() {
  const { args, target, built, shipped } = plan(process.env);
  const run = spawnSync(process.execPath, args, {
    cwd: root, stdio: 'inherit', windowsHide: true, shell: false, env: { ...process.env, CARGO_TARGET_DIR: target },
  });
  if (run.error) throw run.error;
  if (run.status !== 0) throw new Error(`CLI build failed: ${run.status ?? run.signal}`);
  mkdirSync(path.dirname(shipped), { recursive: true });
  copyFileSync(built, shipped);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
