import { createInterface } from 'node:readline';
import type { EngineEvent, Session } from '../events.ts';
import { realSpawn, stderrTail, type Spawner } from '../proc.ts';
import { claudeArgs, type ClaudeLaunch } from './args.ts';
import { parseClaudeMessage } from './parse.ts';

export function startClaude(
  opts: { bin: string; cwd: string; launch: ClaudeLaunch; spawn?: Spawner },
  onEvent: (event: EngineEvent) => void,
): Session {
  const proc = (opts.spawn ?? realSpawn)(opts.bin, claudeArgs(opts.launch), opts.cwd);
  const tail = stderrTail(proc);
  const pending = new Map<string, unknown>();
  let closed = false;
  let controlId = 0;
  const write = (msg: object) => proc.stdin.write(JSON.stringify(msg) + '\n');

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
    if (!closed) onEvent({ type: 'error', message: `Claude Code stopped (exit ${code}). ${tail()}`.trim(), fatal: true });
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
      closed = true;
      proc.stdin.end();
      proc.kill();
    },
  };
}
