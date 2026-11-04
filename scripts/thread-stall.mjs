// Run the focused measurement in the already-built, hidden test app.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { join, resolve } from 'node:path';

if (process.argv.length !== 2) throw new Error('Run without arguments against the already-built hidden test app');
const target = resolve(process.env.CARGO_TARGET_DIR || 'target');
const driver = join(target, `debug/examples/ux_audit${process.platform === 'win32' ? '.exe' : ''}`);
const child = spawn(driver, ['--codex'], { windowsHide: true, stdio: ['pipe', 'pipe', 'inherit'] });
const closed = new Promise((done, fail) => { child.once('error', fail); child.once('close', done); });
const lines = createInterface({ input: child.stdout });
let measured = false, clean = false;
let timedOut = false;
const timeout = setTimeout(() => {
  timedOut = true;
  child.stdin.end('{"op":"quit"}\n');
  setTimeout(() => child.kill(), 5000).unref();
}, 120_000);
try {
  for await (const line of lines) {
    if (line === 'READY') {
      child.stdin.write(JSON.stringify({ op: 'eval', value: `(async () => {
        const checks = await window.__faberChecks(undefined, 'thread stall: open and scroll 300 messages');
        return { checks, ms: window.threadStallMs };
      })()` }) + '\n');
    } else if (line === 'CLEAN') {
      clean = true;
    } else {
      const reply = JSON.parse(line);
      if (reply.error) throw new Error(reply.error);
      const { checks, ms } = reply.ok;
      if (checks.length !== 1 || !checks[0].ok || !Number.isFinite(ms)) throw new Error(JSON.stringify(checks));
      console.log(`300-message open and scroll longest timer delay: ${ms.toFixed(1)} ms`);
      measured = true;
      child.stdin.write('{"op":"quit"}\n');
    }
  }
  const code = await closed;
  if (timedOut || code !== 0 || !measured || !clean) throw new Error(`test app did not finish cleanly (exit ${code}, timeout ${timedOut})`);
} catch (error) {
  if (!child.stdin.destroyed) child.stdin.end('{"op":"quit"}\n');
  await closed.catch(() => {});
  throw error;
} finally {
  clearTimeout(timeout);
}
