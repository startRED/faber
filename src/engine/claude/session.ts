import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import type { Readable } from 'node:stream';
import type { EngineEvent, UserTurn } from '../events.ts';
import { claudeArgs, type ClaudeLaunch } from './args.ts';
import { parseClaudeMessage } from './parse.ts';

// The slice of a child process the session needs, so tests can replay a recording without spawning anything.
export interface Proc {
  stdin: { write(chunk: string): unknown; end(): unknown };
  stdout: Readable;
  stderr: Readable;
  kill(): unknown;
  once(event: 'exit', listener: (code: number | null) => void): unknown;
}
export type Spawner = (bin: string, args: string[], cwd: string) => Proc;

const realSpawn: Spawner = (bin, args, cwd) =>
  spawn(bin, args, { cwd, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });

export interface ClaudeSession {
  send(turn: UserTurn): void;
  answer(requestId: string, allow: boolean): void;
  interrupt(): void;
  close(): void;
}

export function startClaude(
  opts: { bin: string; cwd: string; launch: ClaudeLaunch; spawn?: Spawner },
  onEvent: (event: EngineEvent) => void,
): ClaudeSession {
  const proc = (opts.spawn ?? realSpawn)(opts.bin, claudeArgs(opts.launch), opts.cwd);
  const pending = new Map<string, unknown>();
  let stderrTail = '';
  let finished = false;
  let controlId = 0;
  const write = (msg: object) => proc.stdin.write(JSON.stringify(msg) + '\n');

  proc.stderr.on('data', (chunk: Buffer) => {
    stderrTail = (stderrTail + chunk.toString()).slice(-2000);
  });
  createInterface({ input: proc.stdout }).on('line', (line) => {
    if (!line.trim()) return;
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      onEvent({ type: 'error', message: `Unexpected output from Claude Code: ${line.slice(0, 200)}` });
      return;
    }
    for (const event of parseClaudeMessage(msg)) {
      if (event.type === 'permission') pending.set(event.requestId, event.input);
      onEvent(event);
    }
  });
  proc.once('exit', (code) => {
    if (!finished) onEvent({ type: 'error', message: `Claude Code stopped (exit ${code}). ${stderrTail.trim()}`.trim() });
  });

  return {
    send(turn) {
      const content = turn.images?.length
        ? [
            ...turn.images.map((i) => ({ type: 'image', source: { type: 'base64', media_type: i.mediaType, data: i.base64 } })),
            { type: 'text', text: turn.text },
          ]
        : turn.text;
      write({ type: 'user', message: { role: 'user', content }, parent_tool_use_id: null, session_id: '' });
    },
    answer(requestId, allow) {
      const input = pending.get(requestId);
      pending.delete(requestId);
      write({
        type: 'control_response',
        response: {
          subtype: 'success',
          request_id: requestId,
          response: allow ? { behavior: 'allow', updatedInput: input } : { behavior: 'deny', message: 'The user declined.' },
        },
      });
    },
    interrupt() {
      write({ type: 'control_request', request_id: `faber-${++controlId}`, request: { subtype: 'interrupt' } });
    },
    close() {
      finished = true;
      proc.stdin.end();
      proc.kill();
    },
  };
}
