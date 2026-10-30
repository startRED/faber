// The Rust host in its own process, spoken to in newline-delimited JSON (`host/src/wire.rs`). Phase 1 of R459 measures
// it against the same host loaded as a native addon; this one keeps the window alive if the engine ever panics.

import { type ChildProcessByStdio, spawn } from 'node:child_process';
import path from 'node:path';
import { createInterface } from 'node:readline';
import type { Readable, Writable } from 'node:stream';

type Pending = { resolve: (value: unknown) => void; reject: (reason: Error) => void };

/** Where the host program is: beside the app once installed, or the one `FABER_SIDECAR` names while developing. */
function program(): string {
  if (process.env.FABER_SIDECAR) return process.env.FABER_SIDECAR;
  const exe = process.platform === 'win32' ? 'faber-sidecar.exe' : 'faber-sidecar';
  return path.join(process.resourcesPath, exe);
}

export class Sidecar {
  private next = 1;
  private alive = true;
  private readonly pending = new Map<number, Pending>();

  private readonly child: ChildProcessByStdio<Writable, Readable, null>;

  private constructor(child: ChildProcessByStdio<Writable, Readable, null>) {
    this.child = child;
  }

  static start(onEvent: (event: unknown) => void): Sidecar {
    // No shell, no console window: the host is a plain child of the app.
    const child = spawn(program(), [], { stdio: ['pipe', 'pipe', 'inherit'], windowsHide: true, shell: false });
    const sidecar = new Sidecar(child);
    const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
    lines.on('line', (line) => sidecar.heard(line, onEvent));
    const gone = () => {
      sidecar.alive = false;
      for (const { reject } of sidecar.pending.values()) reject(new Error('error.host.gone'));
      sidecar.pending.clear();
    };
    child.on('exit', gone);
    // A program that cannot start (missing, not allowed) emits `error` instead of `exit`; unheard, it would end the app.
    child.on('error', (error) => {
      console.error(`faber: the host could not start: ${error.message}`);
      gone();
    });
    // Writing to a host that just ended fails here, not as an uncaught error; its calls were already answered above.
    child.stdin.on('error', () => {});
    return sidecar;
  }

  call(cmd: string, args: Record<string, unknown>): Promise<unknown> {
    if (!this.alive) return Promise.reject(new Error('error.host.gone'));
    const id = this.next++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(`${JSON.stringify({ id, cmd, args })}\n`);
    });
  }

  async stop(): Promise<void> {
    this.child.stdin.end();
  }

  private heard(line: string, onEvent: (event: unknown) => void): void {
    let message: { id?: number; ok?: unknown; err?: string; event?: unknown };
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if ('event' in message) {
      onEvent(message.event);
      return;
    }
    const waiting = message.id === undefined ? undefined : this.pending.get(message.id);
    if (!waiting || message.id === undefined) return;
    this.pending.delete(message.id);
    if (message.err !== undefined) waiting.reject(new Error(message.err));
    else waiting.resolve(message.ok ?? null);
  }
}
