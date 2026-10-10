import { PassThrough } from 'node:stream';

export type Entry = { dir: 'in' | 'out'; msg: Record<string, any> };

// What must match between what Faber sends and what was recorded (the same rule as engine/tests/support).
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

// A fake CLI process that replays a recorded session. Each line Faber writes is checked against the next
// recorded input, then the recorded outputs up to the following input are streamed back.
export function replay(entries: Entry[]) {
  const stdout = new PassThrough();
  const mismatches: string[] = [];
  let at = 0;
  let onExit: (() => void) | undefined;
  let exited = false;
  const exit = () => {
    if (exited) return;
    exited = true;
    stdout.end();
    setImmediate(() => onExit?.());
  };
  const flush = () => {
    while (!exited && at < entries.length && entries[at]!.dir === 'out') stdout.write(JSON.stringify(entries[at++]!.msg) + '\n');
  };
  return {
    mismatches,
    stdout,
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
    once(_event: 'exit', listener: () => void) {
      onExit = listener;
    },
  };
}
