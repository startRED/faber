// Stands in for a CLI binary in end-to-end tests: replays a recorded session over real stdin and stdout.
// usage: node fake-cli.ts <cassette.jsonl> [ignored CLI arguments...]
import { readFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { replay, type Entry } from './replay.ts';

const entries: Entry[] = readFileSync(process.argv[2]!, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
const proc = replay(entries);
proc.stdout.pipe(process.stdout);
createInterface({ input: process.stdin }).on('line', (line) => proc.stdin.write(`${line}\n`));
process.stdin.on('end', () => proc.stdin.end());
proc.once('exit', () => {
  if (proc.mismatches.length) process.stderr.write(`${proc.mismatches.join('\n')}\n`);
  process.exit(0);
});
