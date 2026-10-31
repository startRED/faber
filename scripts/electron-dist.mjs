// Builds the Electron Faber and its installers for this system (R459): the screen, the shell's TypeScript, the release
// sidecar and command line, a staged `out/app` with no dependencies, then electron-builder (installer/). CI runs it; on
// the owner's PC it would make a release build folder of its own, so it is not part of the local proofs.
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(import.meta.dirname, '..');

export function plan(platform = process.platform) {
  const exe = platform === 'win32' ? '.exe' : '';
  return {
    target: path.join(root, 'out', 'electron-target'),
    binaries: ['faber-sidecar', 'faber-cli'].map((name) => `${name}${exe}`),
    builder: platform === 'win32' ? '--win' : platform === 'darwin' ? '--mac' : '--linux',
    cargo: ['build', '--release', '-p', 'faber-sidecar', '-p', 'faber-cli', '--locked'],
    triples: platform === 'darwin' ? ['aarch64-apple-darwin', 'x86_64-apple-darwin'] : [],
  };
}

function run(args, env = process.env) {
  const result = spawnSync(process.execPath, args, { cwd: root, env, stdio: 'inherit', windowsHide: true, shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Command failed: ${args[0]} (${result.status})`);
}

function main() {
  const build = plan();
  const npm = process.env.npm_execpath || path.join(path.dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js');
  run([npm, 'run', 'build']);
  run([npm, 'run', 'electron:build']);
  const targets = build.triples.length ? build.triples : [''];
  const bin = path.join(root, 'out/electron-bin');
  mkdirSync(bin, { recursive: true });
  for (const triple of targets) {
    run(['scripts/cargo.mjs', ...build.cargo, ...(triple ? ['--target', triple] : [])], { ...process.env, CARGO_TARGET_DIR: build.target });
    const arch = triple ? (triple.startsWith('aarch64') ? 'arm64' : 'x64') : process.arch;
    const destination = path.join(bin, arch);
    mkdirSync(destination, { recursive: true });
    for (const name of build.binaries) cpSync(path.join(build.target, ...(triple ? [triple] : []), 'release', name), path.join(destination, name));
  }
  const app = path.join(root, 'out/app');
  rmSync(app, { recursive: true, force: true });
  mkdirSync(path.join(app, 'out'), { recursive: true });
  const pkg = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8'));
  const { version, author, description } = pkg;
  writeFileSync(path.join(app, 'package.json'), JSON.stringify({ name: 'faber', productName: 'Faber', version, type: 'module', main: 'out/electron/main.js', author, description }, null, 2));
  cpSync(path.join(root, 'out/electron'), path.join(app, 'out/electron'), { recursive: true });
  cpSync(path.join(root, 'dist'), path.join(app, 'dist'), { recursive: true });
  run(['installer/node_modules/electron-builder/cli.js', '--config', path.join(root, 'installer/electron-builder.config.cjs'), build.builder]);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
