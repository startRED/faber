import { readFileSync } from 'node:fs';
import { PassThrough } from 'node:stream';
import type { Proc } from '../../src/engine/claude/session.ts';

export type Entry = { dir: 'in' | 'out'; msg: Record<string, any> };

export function readCassette(path: string): Entry[] {
  return readFileSync(path, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
}

// What must match between what the driver sends and what was recorded.
function key(msg: Record<string, any>): string {
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
