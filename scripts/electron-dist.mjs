// Builds the Electron Faber and its installers for this system (R459): the screen, the shell's TypeScript, the release
// sidecar and command line, a staged `out/app` with no dependencies, then electron-builder (installer/). CI runs it; on
// the owner's PC it would make a release build folder of its own, so it is not part of the local proofs.
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(import.meta.dirname, '..');

// The command line ships beside the app once the workspace has it (`cli/`, R459). On Windows the release binaries link
// the C runtime statically: a fresh Windows has no Visual C++ runtime, and without it the sidecar dies at load with
// 0xC0000135 before Faber can draw anything. Only this release build gets the flag, so local build caches stay valid.
const STATIC_CRT = '-C target-feature=+crt-static';
export function plan(platform = process.platform, withCli = existsSync(path.join(root, 'cli/Cargo.toml')), base = process.env) {
  const exe = platform === 'win32' ? '.exe' : '';
  const packages = withCli ? ['faber-sidecar', 'faber-cli'] : ['faber-sidecar'];
  return {
    target: path.join(root, 'out', 'electron-target'),
    binaries: packages.map((name) => `${name}${exe}`),
    builder: platform === 'win32' ? '--win' : platform === 'darwin' ? '--mac' : '--linux',
    cargo: ['build', '--release', ...packages.flatMap((name) => ['-p', name]), '--locked'],
    triples: platform === 'darwin' ? ['aarch64-apple-darwin', 'x86_64-apple-darwin'] : [],
    // RUSTFLAGS, when set, wins over the per-target variables, so the flag joins it rather than vanish.
    env: platform === 'win32'
      ? {
          CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS: STATIC_CRT,
          CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_RUSTFLAGS: STATIC_CRT,
          ...(base.RUSTFLAGS ? { RUSTFLAGS: `${base.RUSTFLAGS} ${STATIC_CRT}` } : {}),
        }
      : {},
  };
}

/** The C runtime libraries a Windows executable imports (normal and delayed imports); a release binary must list none. */
export function runtimeImports(bytes) {
  const pe = bytes.readUInt32LE(0x3c);
  const optional = pe + 24;
  const directories = optional + (bytes.readUInt16LE(optional) === 0x20b ? 112 : 96);
  const table = optional + bytes.readUInt16LE(pe + 20);
  const sections = Array.from({ length: bytes.readUInt16LE(pe + 6) }, (_, i) => ({
    address: bytes.readUInt32LE(table + i * 40 + 12),
    size: Math.max(bytes.readUInt32LE(table + i * 40 + 8), bytes.readUInt32LE(table + i * 40 + 16)),
    raw: bytes.readUInt32LE(table + i * 40 + 20),
  }));
  const offset = (rva) => {
    const section = sections.find((s) => rva >= s.address && rva < s.address + s.size);
    if (!section) throw new Error(`address 0x${rva.toString(16)} is in no section`);
    return rva - section.address + section.raw;
  };
  const text = (rva) => {
    const start = offset(rva);
    return bytes.toString('latin1', start, bytes.indexOf(0, start));
  };
  const names = (rva, step, field) => {
    const found = [];
    for (let at = rva ? offset(rva) : -1; at >= 0 && bytes.readUInt32LE(at + field); at += step) found.push(text(bytes.readUInt32LE(at + field)));
    return found;
  };
  const all = [...names(bytes.readUInt32LE(directories + 8), 20, 12), ...names(bytes.readUInt32LE(directories + 13 * 8), 32, 4)];
  return all.filter((name) => /^(vcruntime|msvcp|ucrtbase|api-ms-win-crt-)/i.test(name));
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
    run(['scripts/cargo.mjs', ...build.cargo, ...(triple ? ['--target', triple] : [])], { ...process.env, ...build.env, CARGO_TARGET_DIR: build.target });
    const arch = triple ? (triple.startsWith('aarch64') ? 'arm64' : 'x64') : process.arch;
    const destination = path.join(bin, arch);
    mkdirSync(destination, { recursive: true });
    for (const name of build.binaries) {
      const built = path.join(build.target, ...(triple ? [triple] : []), 'release', name);
      // A fresh Windows has no Visual C++ runtime: a binary that needs one would never start there (0xC0000135).
      const needs = process.platform === 'win32' ? runtimeImports(readFileSync(built)) : [];
      if (needs.length) throw new Error(`${name} needs ${needs.join(', ')}, which a fresh Windows lacks`);
      cpSync(built, path.join(destination, name));
    }
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
