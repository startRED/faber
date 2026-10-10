import { spawn } from 'node:child_process';
import type { Readable } from 'node:stream';

// The slice of a child process a driver needs, so tests can replay a recording without spawning anything.
export interface Proc {
  stdin: { write(chunk: string): unknown; end(): unknown };
  stdout: Readable;
  stderr: Readable;
  kill(): unknown;
  once(event: 'exit', listener: (code: number | null) => void): unknown;
}

export type Spawner = (bin: string, args: string[], cwd: string) => Proc;

export const realSpawn: Spawner = (bin, args, cwd) => {
  const child = spawn(bin, args, { cwd, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  // A missing binary emits 'error' instead of 'exit'; report it the same way so it never crashes the app.
  child.on('error', (err) => {
    child.stderr.emit('data', Buffer.from(String(err)));
    child.emit('exit', null);
  });
  child.stdin.on('error', () => {});
  return child;
};

// Keeps the end of stderr so a crash can say why.
export function stderrTail(proc: Proc): () => string {
  let tail = '';
  proc.stderr.on('data', (chunk: Buffer) => {
    tail = (tail + chunk.toString()).slice(-2000);
  });
  return () => tail.trim();
}
