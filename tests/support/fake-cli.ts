// Stands in for a CLI binary in end-to-end tests: replays a recorded session over real stdin and stdout.
// usage: node fake-cli.ts <cassette.jsonl> [ignored CLI arguments...]
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createInterface } from 'node:readline';
import { replay, type Entry } from './replay.ts';

// Setup recordings describe commands as well as the streaming conversation. State is a boolean
// test marker, never a credential; each scenario supplies its own directory and recordings.
type SetupRecording = {
  vendor: 'claude' | 'codex';
  version: string;
  chat: string;
  models?: string;
};
let cassette = process.argv[2]!;
const args = process.argv.slice(3);
let setup: SetupRecording | undefined;
if (cassette.endsWith('.setup.json')) {
  setup = JSON.parse(readFileSync(cassette, 'utf8')) as SetupRecording;
  const stateDir = process.env.FABER_FAKE_STATE_DIR;
  if (!stateDir) throw new Error('Setup replay requires an isolated FABER_FAKE_STATE_DIR');
  const marker = join(stateDir, `${setup.vendor}.signed-in`);
  const audit = join(stateDir, 'commands.jsonl');
  const status = setup.vendor === 'claude' ? ['auth', 'status'] : ['login', 'status'];
  const login = setup.vendor === 'claude' ? ['auth', 'login'] : ['login'];
  const targets = [audit];
  if (JSON.stringify(args) === JSON.stringify(login)) targets.push(marker);
  appendFileSync(audit, JSON.stringify({ pid: process.pid, ppid: process.ppid, cwd: process.cwd(), vendor: setup.vendor, args, targets }) + '\n');
  // Windows reuses process ids: once this one is gone, its id must not count as a leaked CLI.
  process.on('exit', () => appendFileSync(audit, JSON.stringify({ exited: process.pid }) + '\n'));
  if (args[0] === '--version') {
    process.stdout.write(`${setup.version}\n`);
    process.exit(0);
  }
  if (JSON.stringify(args) === JSON.stringify(status)) {
    const connected = existsSync(marker);
    if (setup.vendor === 'claude') process.stdout.write(JSON.stringify({ loggedIn: connected }) + '\n');
    else process.stderr.write(connected ? 'Logged in using ChatGPT\n' : 'Not logged in\n');
    process.exit(connected ? 0 : 1);
  }
  if (JSON.stringify(args) === JSON.stringify(login)) {
    writeFileSync(marker, 'true');
    process.stdout.write('Fake sign-in complete\n');
    process.exit(0);
  }
  cassette = join(dirname(cassette), setup.chat);
}
let entries: Entry[] = readFileSync(cassette, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
// A short-lived model enumeration and a chat use separate app-server processes. Dispatch on
// their first distinguishing request, without guessing from process order or changing the app.
let selected = setup?.vendor !== 'codex';
let proc = replay(entries);
function connect() {
  proc.stdout.pipe(process.stdout);
  proc.once('exit', () => {
    if (proc.mismatches.length) process.stderr.write(`${proc.mismatches.join('\n')}\n`);
    process.exit(setup && proc.mismatches.length ? 1 : 0);
  });
}
connect();
createInterface({ input: process.stdin }).on('line', (line) => {
  if (!selected) {
    const request = JSON.parse(line);
    if (request.method === 'initialize' || request.method === 'initialized') {
      // Initialization must answer before the caller can send its distinguishing request.
      if (request.method === 'initialize') {
        process.stdout.write(JSON.stringify({ id: request.id, result: { userAgent: 'faber-setup-replay', platformFamily: 'windows', platformOs: 'windows' } }) + '\n');
      }
      return;
    }
    if (request.method === 'model/list' && setup?.models) {
      entries = readFileSync(join(dirname(process.argv[2]!), setup.models), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
    }
    // Initialization was answered above; start the cassette after its handshake.
    proc.stdout.unpipe(process.stdout);
    const handshake = entries.findIndex((entry) => entry.dir === 'in' && entry.msg.method === 'initialized');
    proc = replay(entries.slice(handshake + 1));
    connect();
    selected = true;
    proc.stdin.write(line + '\n');
    return;
  }
  proc.stdin.write(`${line}\n`);
});
process.stdin.on('end', () => proc.stdin.end());
