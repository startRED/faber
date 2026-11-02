import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { notesBase, parse } from './channel.mjs';

export function guard(tag, git, readGate) {
  const version = parse(tag ?? '');
  if (!version) throw new Error('expected a release tag');
  if (version.preview !== null) return;
  const gate = JSON.parse(readGate());
  if (gate?.allowed !== true || Object.keys(gate).length !== 1) throw new Error('Electron stable releases are disabled');
  const previous = notesBase(tag, git('tag', '--list').trim().split(/\r?\n/), () => '');
  if (!previous) throw new Error('previous stable tag not found');
  if (git('diff', '--name-only', `${previous}..${tag}`, '--', 'release/electron-stable.json').trim()) {
    throw new Error('Electron stable switch changed in the release range');
  }
}

if (process.argv[1]?.endsWith('electron-stable-guard.mjs')) {
  try {
    if (process.argv.length !== 3) throw new Error('expected a release tag');
    guard(process.argv[2], (...args) => execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }),
      () => readFileSync(new URL('../release/electron-stable.json', import.meta.url), 'utf8'));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
