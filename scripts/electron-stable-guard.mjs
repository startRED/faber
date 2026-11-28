import { readFileSync } from 'node:fs';
import { parse } from './channel.mjs';

// The committed switch is the owner's authorization for a stable Electron release (R529); he turned it on for 1.0.
export function guard(tag, readGate) {
  const version = parse(tag ?? '');
  if (!version) throw new Error('expected a release tag');
  if (version.preview !== null) return;
  const gate = JSON.parse(readGate());
  if (gate?.allowed !== true || Object.keys(gate).length !== 1) throw new Error('Electron stable releases are disabled');
}

if (process.argv[1]?.endsWith('electron-stable-guard.mjs')) {
  try {
    if (process.argv.length !== 3) throw new Error('expected a release tag');
    guard(process.argv[2], () => readFileSync(new URL('../release/electron-stable.json', import.meta.url), 'utf8'));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
