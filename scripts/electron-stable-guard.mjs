import { readFileSync } from 'node:fs';
import { parse } from './channel.mjs';

try {
  const tag = process.argv[2];
  const version = parse(tag ?? '');
  if (!version || process.argv.length !== 3) throw new Error('expected a release tag');
  if (version.preview === null) {
    const gate = JSON.parse(readFileSync(new URL('../release/electron-stable.json', import.meta.url), 'utf8'));
    if (gate?.allowed !== true || Object.keys(gate).length !== 1) throw new Error('Electron stable releases are disabled');
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
