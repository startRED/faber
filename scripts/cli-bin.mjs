// Builds the command line (`cli/`) for the platform being packaged and puts it where the installer takes it from:
// `src-tauri/binaries/faber.exe`, installed as `bin\faber.exe` beside the app (src-tauri/tauri.cli.conf.json). Run by
// `tauri build` before it builds the app, so TAURI_ENV_TARGET_TRIPLE names the platform; never through a shell.
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

/** The cargo arguments and the built file for one packaging run. */
export function plan(env, platform = process.platform) {
  const triple = env.TAURI_ENV_TARGET_TRIPLE || '';
  if (triple && !/^[a-z0-9_]+(-[a-z0-9_.]+){1,4}$/.test(triple)) throw new Error(`Unexpected target: ${triple}`);
  const windows = triple ? triple.includes('windows') : platform === 'win32';
  const debug = env.TAURI_ENV_DEBUG === 'true';
  const target = env.CARGO_TARGET_DIR ? path.resolve(env.CARGO_TARGET_DIR) : path.join(root, 'target');
  const exe = windows ? '.exe' : '';
  return {
    args: ['build', '-p', 'faber-cli', '--locked', ...(debug ? [] : ['--release']), ...(triple ? ['--target', triple] : [])],
    built: path.join(target, ...(triple ? [triple] : []), debug ? 'debug' : 'release', `faber-cli${exe}`),
    shipped: path.join(root, 'src-tauri', 'binaries', `faber${exe}`),
  };
}

function main() {
  const { args, built, shipped } = plan(process.env);
  const run = spawnSync('cargo', args, { cwd: root, stdio: 'inherit', windowsHide: true });
  if (run.error) throw run.error;
  if (run.status !== 0) process.exit(run.status ?? 1);
  mkdirSync(path.dirname(shipped), { recursive: true });
  copyFileSync(built, shipped);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
