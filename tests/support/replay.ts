import { readFileSync } from 'node:fs';
import { PassThrough } from 'node:stream';
import { fileURLToPath } from 'node:url';
import type { EngineEvent, Session } from '../../src/engine/events.ts';
import type { Proc, Spawner } from '../../src/engine/proc.ts';

export type Entry = { dir: 'in' | 'out'; msg: Record<string, any> };

export function fixture(vendor: string, name: string): Entry[] {
  const path = fileURLToPath(new URL(`../fixtures/${vendor}/${name}.jsonl`, import.meta.url));
  return readFileSync(path, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
}

export const textOf = (events: EngineEvent[]) => events.map((e) => (e.type === 'text' ? e.delta : '')).join('');

// Runs a recorded session through a real driver. `react` answers like the app would and returns true when done.
export function drive(
  entries: Entry[],
  start: (spawn: Spawner, onEvent: (e: EngineEvent) => void) => Session,
  first: string,
  react: (e: EngineEvent, s: Session) => boolean,
) {
  const proc = replay(entries);
  const events: EngineEvent[] = [];
  return new Promise<{ events: EngineEvent[]; mismatches: string[] }>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`stalled after ${JSON.stringify(events.at(-1))}; ${proc.mismatches}`)), 2000);
    const session = start(() => proc, (e) => {
      events.push(e);
      if (react(e, session)) {
        clearTimeout(timer);
        session.close();
        resolve({ events, mismatches: proc.mismatches });
      }
    });
    session.send({ text: first });
  });
}

// What must match between what the driver sends and what was recorded.
function key(msg: Record<string, any>): string {
  // Codex app-server speaks JSON-RPC: requests and notifications have a method, answers have an id.
  if (msg.method === 'turn/start') return `turn/start ${msg.id} ${JSON.stringify(msg.params?.input)}`;
  if (msg.method) return `${msg.method}${msg.id === undefined ? '' : ` ${msg.id}`}`;
  if (msg.id !== undefined) return `answer ${msg.id} ${msg.result?.decision ?? ''}`;
  if (msg.type === 'user') return `user ${JSON.stringify(msg.message?.content)}`;
  if (msg.type === 'control_request') return `control_request ${msg.request?.subtype}`;
  if (msg.type === 'control_response') {
    const r = msg.response;
    return `control_response ${r?.request_id} ${r?.response?.behavior ?? 'ok'}`;
  }
  return String(msg.type);
}

// A fake CLI process that replays a recorded session. Each time the driver writes a line, it is checked
// against the next recorded input, then the recorded outputs up to the following input are streamed back.
export function replay(entries: Entry[]) {
  const stdout = new PassThrough();
  const stderr = new PassThrough();
  const mismatches: string[] = [];
  let at = 0;
  let onExit: ((code: number | null) => void) | undefined;
  let exited = false;
  const exit = () => {
    if (exited) return;
    exited = true;
    stdout.end();
    setImmediate(() => onExit?.(0));
  };
  const flush = () => {
    while (!exited && at < entries.length && entries[at]!.dir === 'out') stdout.write(JSON.stringify(entries[at++]!.msg) + '\n');
  };

  const proc: Proc & { mismatches: string[]; remaining: () => number } = {
    mismatches,
    remaining: () => entries.length - at,
    stdin: {
      write(chunk: string) {
        for (const line of chunk.split('\n').filter(Boolean)) {
          const sent = key(JSON.parse(line));
          const expected = entries[at]?.dir === 'in' ? key(entries[at]!.msg) : '(no input expected)';
          if (sent !== expected) mismatches.push(`sent ${sent}, recorded ${expected}`);
          else at++;
          setImmediate(flush);
        }
        return true;
      },
      end: exit,
    },
    stdout,
    stderr,
    kill: exit,
    once(_event, listener) {
      onExit = listener;
      return proc;
    },
  };
  return proc;
}
