import { createInterface } from 'node:readline';
import type { Area } from '../claude/args.ts';
import type { EngineEvent, Session } from '../events.ts';
import { realSpawn, stderrTail, type Spawner } from '../proc.ts';
import { codexArgs, codexThread } from './launch.ts';
import { parseCodexNotification } from './parse.ts';

type Json = Record<string, any>;

export interface CodexLaunch {
  area: Area;
  model?: string;
  effort?: string;
  resume?: string;
}

// One `codex app-server` process per conversation, spoken to over JSON-RPC on stdio.
export function startCodex(
  opts: { bin: string; cwd: string; launch: CodexLaunch; platform?: NodeJS.Platform; spawn?: Spawner },
  onEvent: (event: EngineEvent) => void,
): Session {
  const { area, model, effort, resume } = opts.launch;
  const proc = (opts.spawn ?? realSpawn)(opts.bin, codexArgs(area, opts.platform ?? process.platform), opts.cwd);
  const tail = stderrTail(proc);
  const waiting = new Map<number, (msg: Json) => void>();
  const approvals = new Map<string, number | string>();
  const queued: (() => void)[] = [];
  const tokens = { input: 0, output: 0, streamed: new Set<string>() };
  let nextId = 0;
  let threadId: string | undefined;
  let turnId: string | undefined;
  let closed = false;

  const write = (msg: object) => proc.stdin.write(JSON.stringify(msg) + '\n');
  const request = (method: string, params: object, onResult?: (result: Json) => void) => {
    const id = ++nextId;
    waiting.set(id, (msg) => {
      if (msg.error) onEvent({ type: 'error', message: `Codex ${method} failed: ${msg.error.message ?? JSON.stringify(msg.error)}` });
      else onResult?.(msg.result);
    });
    write({ id, method, params });
  };

  createInterface({ input: proc.stdout }).on('line', (line) => {
    if (!line.trim()) return;
    let msg: Json;
    try {
      msg = JSON.parse(line);
    } catch {
      onEvent({ type: 'error', message: `Unexpected output from Codex: ${line.slice(0, 200)}` });
      return;
    }
    if (msg.method === undefined) {
      waiting.get(msg.id)?.(msg);
      waiting.delete(msg.id);
    } else if (msg.id !== undefined) {
      if (String(msg.method).endsWith('requestApproval')) {
        approvals.set(String(msg.id), msg.id);
        const p = msg.params ?? {};
        onEvent({ type: 'permission', requestId: String(msg.id), tool: p.kind ?? msg.method, input: p.command ?? p.changes ?? p });
      } else {
        write({ id: msg.id, error: { code: -32601, message: `Faber does not handle ${msg.method} yet` } });
      }
    } else {
      if (msg.method === 'turn/started') turnId = msg.params?.turn?.id;
      for (const event of parseCodexNotification(msg, tokens)) onEvent(event);
    }
  });
  proc.once('exit', (code) => {
    if (!closed) onEvent({ type: 'error', message: `Codex stopped (exit ${code}). ${tail()}`.trim(), fatal: true });
  });

  request('initialize', { clientInfo: { name: 'faber', title: 'Faber', version: '0.0.0' }, capabilities: { experimentalApi: false, requestAttestation: false } }, () => {
    write({ method: 'initialized' });
    const thread = codexThread(area, model);
    const [method, params] = resume ? ['thread/resume', { threadId: resume, ...thread }] : ['thread/start', { cwd: opts.cwd, ...thread }];
    request(method, params, (result) => {
      threadId = result.thread.id;
      onEvent({ type: 'started', sessionId: result.thread.id, model: result.thread.model ?? model ?? 'codex' });
      for (const run of queued.splice(0)) run();
    });
  });

  return {
    send(turn) {
      const input = [
        ...(turn.images ?? []).map((i) => ({ type: 'image', url: `data:${i.mediaType};base64,${i.base64}` })),
        { type: 'text', text: turn.text, text_elements: [] },
      ];
      const run = () => request('turn/start', { threadId, input, ...(effort ? { effort } : {}) });
      if (threadId) run();
      else queued.push(run);
    },
    answer(requestId, allow) {
      const id = approvals.get(requestId);
      approvals.delete(requestId);
      if (id !== undefined) write({ id, result: { decision: allow ? 'accept' : 'decline' } });
    },
    interrupt() {
      if (threadId && turnId) request('turn/interrupt', { threadId, turnId });
    },
    close() {
      closed = true;
      proc.stdin.end();
      proc.kill();
    },
  };
}
