// The Rust host in its own process, spoken to in newline-delimited JSON (`host/src/wire.rs`). Phase 1 of R459 measures
// it against the same host loaded as a native addon; this one keeps the window alive if the engine ever panics.

import { type ChildProcessByStdio, spawn } from 'node:child_process';
import path from 'node:path';
import { createInterface } from 'node:readline';
import type { Readable, Writable } from 'node:stream';
import { app } from 'electron';
import { LOGIN_ITEM } from './orders.ts';
import type { PaneNote } from './pane.ts';

/** What the host says on its own: events for the screen, and orders for the shell (`Order` in `host/src/wire.rs`). */
export type Listeners = { event: (event: unknown) => void; order: (order: unknown) => void };

/** A failure crosses as the host sends it: an error key, or `{key, detail}` for the few that name a field. */
type Pending = { resolve: (value: unknown) => void; reject: (failure: unknown) => void };

/** The longest request line the host reads (`MAX_LINE` in `host/src/wire.rs`): a longer one is refused here, not there. */
const MAX_LINE = 16 * 1024 * 1024;
/** Calls waiting for an answer at once. The screen makes a handful; thousands mean a page gone wrong, refused here. */
const MAX_PENDING = 1024;
/** Bytes written to the host it has not read yet, past which new calls are refused instead of queued. */
const MAX_UNREAD = 4 * MAX_LINE;

/** Where the host program is: beside the app once installed, or, while developing only, the one `FABER_SIDECAR` names. */
function program(): string {
  if (!app.isPackaged && process.env.FABER_SIDECAR) return process.env.FABER_SIDECAR;
  const exe = process.platform === 'win32' ? 'faber-sidecar.exe' : 'faber-sidecar';
  return path.join(process.resourcesPath, exe);
}

/** Whether Faber's own startup entry (`LOGIN_ITEM`, the name the Tauri Faber used too) is there and turned on. */
function startsWithSystem(): boolean {
  const settings = app.getLoginItemSettings({ args: ['--autostart'] });
  if (process.platform !== 'win32') return settings.openAtLogin;
  return settings.launchItems.some((item) => item.name === LOGIN_ITEM && item.enabled);
}

export class Sidecar {
  private next = 1;
  private alive = true;
  private readonly pending = new Map<number, Pending>();
  private readonly child: ChildProcessByStdio<Writable, Readable, null>;
  private readonly exited: Promise<void>;

  private constructor(child: ChildProcessByStdio<Writable, Readable, null>, exited: Promise<void>) {
    this.child = child;
    this.exited = exited;
  }

  static start(listen: Listeners): Sidecar {
    // What only the app knows (`faber-sidecar`'s own doc): its version, its process (whose windows are Faber's) and
    // whether it starts with the system.
    const env = {
      ...process.env,
      FABER_VERSION: app.getVersion(),
      FABER_SHELL_PID: String(process.pid),
      FABER_AUTOSTART: app.isPackaged && startsWithSystem() ? '1' : '0',
      // Faber's own browser pane: the shell has views for it, except in a test or automated run (hidden), which shows none.
      ...(process.env.FABER_HIDDEN === '1' ? {} : { FABER_PANE: '1' }),
      // Self-updates (`faber-update`) run only in the installed app, which says where its program is for the installer.
      ...(app.isPackaged ? { FABER_UPDATES: '1', FABER_INSTALL_DIR: path.dirname(process.execPath) } : {}),
    };
    // No shell, no console window: the host is a plain child of the app.
    // A hidden run never has a pane, whatever the environment it was started in says.
    if (process.env.FABER_HIDDEN === '1') delete env.FABER_PANE;
    const child = spawn(program(), [], { stdio: ['pipe', 'pipe', 'inherit'], windowsHide: true, shell: false, env });
    let ended: () => void = () => {};
    const sidecar = new Sidecar(
      child,
      new Promise((resolve) => {
        ended = resolve;
      }),
    );
    const gone = () => {
      sidecar.alive = false;
      for (const { reject } of sidecar.pending.values()) reject(new Error('error.host.gone'));
      sidecar.pending.clear();
      ended();
    };
    child.on('exit', gone);
    // A program that cannot start (missing, not allowed) emits `error` instead of `exit`; unheard, it would end the app.
    child.on('error', (error) => {
      console.error(`faber: the host could not start: ${error.message}`);
      gone();
    });
    // Writing to a host that just ended fails here, not as an uncaught error; its calls were already answered above.
    child.stdin.on('error', () => {});
    const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
    lines.on('line', (line) => sidecar.heard(line, listen));
    return sidecar;
  }

  call(cmd: string, args: Record<string, unknown>): Promise<unknown> {
    if (!this.alive) return Promise.reject(new Error('error.host.gone'));
    if (this.pending.size >= MAX_PENDING) return Promise.reject(new Error('error.host.busy'));
    const id = this.next++;
    let line: string;
    try {
      line = `${JSON.stringify({ id, cmd, args })}\n`;
    } catch {
      return Promise.reject(new Error('error.host.badArgs'));
    }
    if (Buffer.byteLength(line) > MAX_LINE) return Promise.reject(new Error('error.host.badRequest'));
    // A host that stopped reading does not make the app buffer without end.
    if (this.child.stdin.writableLength > MAX_UNREAD) return Promise.reject(new Error('error.host.busy'));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(line);
    });
  }

  /** Tells the host what only the app sees (`ShellNote` in `host/src/wire.rs`); it answers nothing. */
  note(
    note:
      | { kind: 'focus'; focused: boolean }
      | { kind: 'window'; visible: boolean }
      | { kind: 'picked'; id: number; path: string | null; noWindow: boolean }
      | PaneNote,
  ): void {
    if (this.alive && !this.child.stdin.writableEnded) this.child.stdin.write(`${JSON.stringify({ shell: note })}\n`);
  }

  /** Closes the host's input, so it saves what it holds and exits; one still running after `patience` ms is ended. */
  async stop(patience: number): Promise<void> {
    if (!this.alive) return;
    this.child.stdin.end();
    let timer: NodeJS.Timeout | undefined;
    const late = new Promise<'late'>((resolve) => {
      timer = setTimeout(() => resolve('late'), patience);
    });
    if ((await Promise.race([this.exited, late])) === 'late') this.child.kill();
    clearTimeout(timer);
  }

  private heard(line: string, listen: Listeners): void {
    let message: { id?: number; ok?: unknown; err?: unknown; event?: unknown; order?: unknown };
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if ('event' in message) {
      listen.event(message.event);
      return;
    }
    if ('order' in message) {
      listen.order(message.order);
      return;
    }
    const waiting = message.id === undefined ? undefined : this.pending.get(message.id);
    if (!waiting || message.id === undefined) return;
    this.pending.delete(message.id);
    if (message.err !== undefined) waiting.reject(message.err);
    else waiting.resolve(message.ok ?? null);
  }
}
