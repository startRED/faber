// What the screen test runner (`testkit/`) asks of the shell, as the Tauri test build answers it (src-tauri/src/main.rs).
// Everything here is behind `app.isPackaged`: an installed Faber ignores every one of these variables, so no test switch
// can reach a person's copy.
//
// - `FABER_CDP_PORT`: the DevTools port the runner drives the page through.
// - `FABER_SOFTWARE_RENDERING=1`: no GPU, for the motion benchmark.
// - `FABER_TEST_DIST`: the folder to serve in place of `dist/` (the Vite build in test mode).
// - `FABER_CHECKS_OUT` (and `FABER_CHECKS_FILTER`): open the page that runs the screen's checks, and write its report there.

import { app, type BrowserWindow, type WebContents } from 'electron';
import { readFileSync, renameSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const env = (name: string): string | undefined => (app.isPackaged ? undefined : process.env[name] || undefined);

/** A run the test runner started: hidden, offscreen, inside a sandbox it made. */
export const testing = !app.isPackaged && process.env.FABER_HIDDEN === '1';
const checksOut = env('FABER_CHECKS_OUT');

/** Called before the app is ready, when Chromium still takes switches and paths. */
export function prepare(): void {
  const port = env('FABER_CDP_PORT');
  if (port) app.commandLine.appendSwitch('remote-debugging-port', port);
  if (testing && env('FABER_SOFTWARE_RENDERING') === '1') app.disableHardwareAcceleration();
  if (testing) {
    // The sentinel measures frames: a window on a desktop no monitor shows may count as hidden, and Chromium then slows or
    // stops animation frames. These ask it not to; the sentinel's clock check proves they held.
    for (const flag of ['disable-renderer-backgrounding', 'disable-backgrounding-occluded-windows', 'disable-background-timer-throttling']) {
      app.commandLine.appendSwitch(flag);
    }
  }
}

/** Chromium features switched off for a test run, appended to the shell's own list. */
export const disabledFeatures: string[] = testing ? ['CalculateNativeWinOcclusion'] : [];

/** The folder the screen is served from. */
export function screenFolder(production: string): string {
  return env('FABER_TEST_DIST') ?? production;
}

/** The page the window opens: the checks page when the runner asked for the checks. */
export function startPage(): string {
  return checksOut ? 'index.html?checks' : 'index.html';
}

/** Offscreen, out of the taskbar and never focused, as the Tauri test build places its window. */
export const placement: Partial<Electron.BrowserWindowConstructorOptions> = testing
  ? { x: -32000, y: -32000, skipTaskbar: true }
  : {};

/**
 * Before the page loads: the checks page must see no reduced-motion preference (a CI runner asks for it) and the
 * runner's name filter. Both are run by Chromium before any script of the page, as an initialization script is.
 * Returns once they are registered.
 */
export async function beforeLoad(contents: WebContents, root: string, origin: string): Promise<void> {
  if (!checksOut) return;
  const filter = env('FABER_CHECKS_FILTER');
  const source =
    readFileSync(path.join(root, 'tests', 'ui', 'system-motion.js'), 'utf8') +
    (filter === undefined ? '' : `\nwindow.__faberChecksFilter = ${JSON.stringify(filter)};`);
  // Stay on the same origin so navigation cannot replace the renderer that owns the debugger's initialization script.
  await contents.loadURL(`${origin}/__test_blank__`);
  contents.debugger.attach('1.3');
  await contents.debugger.sendCommand('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'no-preference' }] });
  await contents.debugger.sendCommand('Page.addScriptToEvaluateOnNewDocument', { source });
}

/** Commands only the test build answers (`#[cfg(feature = "test-hooks")]` in `host/src/table.rs`). */
export function commands(window: () => BrowserWindow | null): Map<string, (args: Record<string, unknown>) => unknown> {
  const own = new Map<string, (args: Record<string, unknown>) => unknown>();
  if (app.isPackaged) return own;
  // The checks' result, written whole before the app ends: the runner polls the file.
  own.set('report_checks', ({ json }) => {
    if (!checksOut) throw new Error('FABER_CHECKS_OUT is not set');
    const part = `${checksOut}.part`;
    writeFileSync(part, String(json));
    renameSync(part, checksOut);
    // Leaving through the normal quit lets the host save and stop before the process ends.
    setImmediate(() => app.quit());
  });
  // Resizes the real hidden window, rather than emulating the page's viewport.
  own.set('test_host_size', ({ width, height }) => {
    if (typeof width !== 'number' || typeof height !== 'number') throw new Error('error.host.badArgs');
    if (!(width >= 720 && width <= 1920 && height >= 600 && height <= 1080)) throw new Error('test host size is outside 720-1920 by 600-1080');
    window()?.setContentSize(width, height);
  });
  return own;
}
