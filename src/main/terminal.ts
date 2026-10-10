import { spawn } from 'node:child_process';
import { chmodSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export interface TerminalCommand {
  command: string;
  args: string[];
  cwd: string;
}

// Characters a shell acts on even inside quotes; paths with accents or parentheses are fine.
const UNSAFE = /["%&|<>^`$!\r\n]/;

// Opens the system terminal running a CLI in its own interface, detached from Faber: the one-click way out when
// Faber cannot follow a CLI's output. Every piece is checked, since the line is built for a shell.
export function openTerminal({ command, args, cwd }: TerminalCommand, platform: NodeJS.Platform = process.platform): void {
  for (const part of [command, ...args, cwd]) if (UNSAFE.test(part)) throw new Error(`Refusing to open a terminal with: ${part}`);
  const quoted = [command, ...args].map((p) => `"${p}"`).join(' ');
  if (platform === 'win32') {
    spawn('cmd.exe', ['/c', `start "Faber" ${quoted}`], { cwd, detached: true, stdio: 'ignore', windowsVerbatimArguments: true }).unref();
  } else if (platform === 'darwin') {
    const script = join(tmpdir(), `faber-${Date.now()}.command`);
    writeFileSync(script, `#!/bin/sh\ncd "${cwd}" && exec ${quoted}\n`);
    chmodSync(script, 0o755);
    spawn('open', ['-a', 'Terminal', script], { detached: true, stdio: 'ignore' }).unref();
  } else {
    spawn('x-terminal-emulator', ['-e', command, ...args], { cwd, detached: true, stdio: 'ignore' }).unref();
  }
}
