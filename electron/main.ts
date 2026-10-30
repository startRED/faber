// Faber's Electron shell (R459): one window, the screen from dist/, and a relay to the Rust host. Nothing here decides
// anything a person sees; the commands, their rules and the events all live in Rust (`host/`). This file only creates
// the window, serves the screen, and carries calls across.

import { app, BrowserWindow, ipcMain, type IpcMainEvent, type IpcMainInvokeEvent, net, protocol, session, shell } from 'electron';
import { createHash } from 'node:crypto';
import { readFileSync, realpathSync, statSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { Sidecar } from './sidecar.ts';
import { apply, type Picked, release } from './orders.ts';
import * as testmode from './testmode.ts';

const SCHEME = 'faber';
const ORIGIN = `${SCHEME}://app`;
/** The screen's colour before it paints, Dark Room's frame (`ui/styles.css`), so the window never flashes white. */
const FRAME = '#101012';
/** If the screen never says it painted, the window still shows after this long (as the Tauri shell does). */
const REVEAL_FAILSAFE_MS = 3000;
/** How long closing waits for the host to save and stop before it is ended anyway. */
const HOST_STOP_MS = 5000;

// Compiled into out/electron/, two folders below the repository.
const root = path.resolve(import.meta.dirname, '..', '..');
// The real folder, so a link inside it can never lead the screen's scheme outside. A test run serves its own build.
const dist = realpathSync(testmode.screenFolder(path.join(root, 'dist')));
const hidden = process.env.FABER_HIDDEN === '1';
const autostart = process.argv.includes('--autostart');

// Every renderer runs in the system sandbox, not only the ones that ask for it.
app.enableSandbox();
// The screen loads only from Faber's own scheme: never file:// and never a remote page.
protocol.registerSchemesAsPrivileged([{ scheme: SCHEME, privileges: { standard: true, secure: true, supportFetchAPI: true } }]);

// Features Faber does not use stay off; each one costs memory in every renderer.
app.commandLine.appendSwitch('disable-features', ['SpareRendererForSitePerProcess', 'MediaRouter', 'Translate', ...testmode.disabledFeatures].join(','));
app.commandLine.appendSwitch('disable-spell-checking');
// The screen fetches nothing from the network (its policy allows only its own scheme), so the network service runs
// inside the main process instead of one of its own: about 10 MB private and 50 MB working set less, measured.
app.commandLine.appendSwitch('enable-features', 'NetworkServiceInProcess2');
// The GPU process is ended on purpose whenever the window closes (`releaseGpu`); that must not count as crashes.
app.commandLine.appendSwitch('disable-gpu-process-crash-limit');

// The screen test runner's switches, ignored by an installed Faber (testmode.ts).
testmode.prepare();

// A second Faber hands its arguments to the first and ends at once, before it starts a host of its own on the same data.
if (!app.requestSingleInstanceLock()) app.exit(0);
// Windows groups notices and the taskbar by this id; the installed Tauri Faber used the same one.
if (process.platform === 'win32') app.setAppUserModelId('io.github.startred.faber');

let win: BrowserWindow | null = null;
let host: Sidecar | null = null;

/**
 * The hashes of the inline scripts dist/index.html ships: only the fixed loader that starts the screen's modules
 * (vite.config.ts). The page may run that loader and what it loads ('strict-dynamic'), never another inline script.
 */
function loaderHashes(): string[] {
  const page = readFileSync(path.join(dist, 'index.html'), 'utf8');
  return [...page.matchAll(/<script>([\s\S]*?)<\/script>/g)].map(
    ([, body]) => `'sha256-${createHash('sha256').update(body ?? '').digest('base64')}'`,
  );
}

/** The same policy the Tauri shell gives the screen, with its own origin in place of Tauri's IPC. */
const CSP = [
  "default-src 'self'",
  `script-src 'self' ${loaderHashes().join(' ')} 'strict-dynamic'`,
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "font-src 'self' data:",
  'media-src data: blob:',
  "frame-src 'none'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join('; ');

/** The file under dist/ a screen address names, or null for anything outside it, malformed, or not a plain file. */
function screenFile(address: string): string | null {
  const url = new URL(address);
  if (url.host !== 'app') return null;
  let relative: string;
  try {
    relative = decodeURIComponent(url.pathname === '/' ? '/index.html' : url.pathname);
  } catch {
    return null;
  }
  try {
    const file = realpathSync(path.join(dist, relative));
    return file.startsWith(dist + path.sep) && statSync(file).isFile() ? file : null;
  } catch {
    return null;
  }
}

/** Serves dist/ under faber://app/. Every answer carries the policy, so no file there can become a page without it. */
function serveScreen(): void {
  protocol.handle(SCHEME, async (request) => {
    const file = screenFile(request.url);
    if (!file) return new Response(null, { status: 404 });
    const response = await net.fetch(pathToFileURL(file).toString());
    const headers = new Headers(response.headers);
    headers.set('Content-Security-Policy', CSP);
    headers.set('X-Content-Type-Options', 'nosniff');
    return new Response(response.body, { status: response.status, headers });
  });
}

/** Only the screen itself speaks to the shell: its window, its top frame, its own origin. */
function fromScreen(event: IpcMainEvent | IpcMainInvokeEvent): boolean {
  const frame = event.senderFrame;
  if (!win || !frame || event.sender !== win.webContents) return false;
  // Two handles on one frame are different objects: the frame is told by its process and routing ids.
  const main = win.webContents.mainFrame;
  if (frame.processId !== main.processId || frame.routingId !== main.routingId) return false;
  try {
    // Node's URL gives a scheme it does not know the origin "null", so the scheme and host are compared instead.
    const url = new URL(frame.url);
    return url.protocol === `${SCHEME}:` && url.host === 'app';
  } catch {
    return false;
  }
}

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 720,
    minHeight: 520,
    show: false,
    frame: process.platform === 'darwin',
    titleBarStyle: process.platform === 'darwin' ? 'hiddenInset' : undefined,
    backgroundColor: FRAME,
    ...testmode.placement,
    webPreferences: {
      preload: path.join(import.meta.dirname, 'preload.cjs'),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      webviewTag: false,
      spellcheck: false,
      // A hidden window keeps throttling its timers, as the system webview did.
      backgroundThrottling: true,
    },
  });
  // The screen opens links through `open_link`; a page never opens a window or leaves its origin by itself.
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  const stay = (event: Electron.Event, url: string) => {
    if (!url.startsWith(`${ORIGIN}/`)) event.preventDefault();
  };
  window.webContents.on('will-navigate', stay);
  window.webContents.on('will-redirect', stay);
  window.webContents.on('will-attach-webview', (event) => event.preventDefault());
  const failsafe = setTimeout(() => reveal(window), REVEAL_FAILSAFE_MS);
  window.once('show', () => clearTimeout(failsafe));
  const resized = () => window.webContents.send('faber:window-resized');
  window.on('maximize', resized);
  window.on('unmaximize', resized);
  window.on('resize', resized);
  // The host decides notices, and a question that comes while the person looks at Faber is never one.
  window.on('focus', () => host?.note({ kind: 'focus', focused: true }));
  window.on('blur', () => host?.note({ kind: 'focus', focused: false }));
  // Closing frees the screen (its renderer and its memory); the host keeps running answers in the background.
  window.on('closed', () => {
    if (win === window) win = null;
    host?.note({ kind: 'focus', focused: false });
    releaseGpu();
    host?.call('quota_attention', { visible: false, focused: false }).catch(() => {});
  });
  const load = () => window.loadURL(`${ORIGIN}/${testmode.startPage()}`);
  if (testmode.testing) {
    // A test run shows its window at once (offscreen, unfocused), as the Tauri test build does: a window never shown
    // can be left unpainted by Chromium, and the screen would wait for its first frame.
    window.showInactive();
    void testmode.beforeLoad(window.webContents, root).then(load);
  } else void load();
  return window;
}

/**
 * With no window open nothing is drawn, yet Chromium keeps its GPU process and the 150 MB or so of buffers it holds
 * (measured on the owner's PC). It is ended here and Chromium starts a fresh one when a window opens again; the crash
 * limit is lifted below so closing to the tray many times never drops the screen to software drawing.
 */
function releaseGpu(): void {
  if (BrowserWindow.getAllWindows().length > 0) return;
  for (const metric of app.getAppMetrics()) {
    if (metric.type === 'GPU') {
      try {
        process.kill(metric.pid);
      } catch {}
    }
  }
}

function reveal(window: BrowserWindow): void {
  if (!hidden && !window.isDestroyed() && !window.isVisible()) window.show();
}

function show(): void {
  if (!win) win = createWindow();
  else {
    if (win.isMinimized()) win.restore();
    win.show();
    win.focus();
  }
}

/** Links in answers open in the person's browser, never inside Faber (as the Tauri shell does). */
async function openLink({ url }: Record<string, unknown>): Promise<void> {
  let link: URL;
  try {
    link = new URL(String(url));
  } catch {
    throw new Error('error.link.web');
  }
  if (link.protocol !== 'https:' && link.protocol !== 'http:') throw new Error('error.link.web');
  try {
    await shell.openExternal(link.toString());
  } catch {
    throw new Error('error.link.open');
  }
}

/** Commands only a window can answer. The rest go to the host, whose command table is the list the screen may call. */
const windowCommands = new Map<string, (args: Record<string, unknown>) => unknown>([
  ...testmode.commands(() => win),
  [
    'screen_ready',
    () => {
      if (win) reveal(win);
    },
  ],
  ['open_link', openLink],
]);

function relay(): void {
  ipcMain.handle('faber:invoke', async (event, command: unknown, args: unknown) => {
    // Any other page (a future browser pane, a frame) is refused before a command runs.
    if (!fromScreen(event)) return { err: 'error.host.notScreen' };
    if (typeof command !== 'string') return { err: 'error.host.badRequest' };
    if (args !== undefined && (args === null || typeof args !== 'object' || Array.isArray(args))) return { err: 'error.host.badArgs' };
    const given = (args ?? {}) as Record<string, unknown>;
    // The answer travels as `{ok}` or `{err}`: a failure is an ordinary answer the screen reads (as Tauri's), not an
    // error Electron would log with a stack for every refused call.
    try {
      const own = windowCommands.get(command);
      if (own) return { ok: (await own(given)) ?? null };
      if (!host) throw new Error('error.host.notReady');
      return { ok: await host.call(command, given) };
    } catch (failure) {
      return { err: failure instanceof Error ? failure.message : failure };
    }
  });
  ipcMain.on('faber:window', (event, action: unknown) => {
    if (!win || !fromScreen(event)) return;
    if (action === 'minimize') win.minimize();
    else if (action === 'toggleMaximize') {
      if (win.isMaximized()) win.unmaximize();
      else win.maximize();
    } else if (action === 'close') win.close();
  });
  ipcMain.handle('faber:window-maximized', (event) => (win && fromScreen(event) ? win.isMaximized() : false));
}

app.on('second-instance', (_event, argv) => {
  if (!argv.includes('--autostart')) show();
});

// Faber stays in the tray when its window closes; only Quit, on the tray's menu, ends it.
app.on('window-all-closed', () => {});

// Quitting waits for the host to save what it holds and stop; one that hangs is ended after a few seconds.
let stopping = false;
app.on('before-quit', (event) => {
  if (stopping || !host) return;
  stopping = true;
  event.preventDefault();
  release();
  void host.stop(HOST_STOP_MS).finally(() => app.quit());
});

void app.whenReady().then(() => {
  // The screen asks for nothing a browser grants: no camera, no notifications from the page, no geolocation, no devices.
  // The exceptions are the clipboard, which Copy and the right-click menu's Paste use (Tauri's window allowed it too).
  const clipboard = (permission: string) => permission === 'clipboard-read' || permission === 'clipboard-sanitized-write';
  session.defaultSession.setPermissionRequestHandler((wc, permission, callback, details) =>
    callback(clipboard(permission) && wc === win?.webContents && details.requestingUrl.startsWith(`${ORIGIN}/`)),
  );
  session.defaultSession.setPermissionCheckHandler((wc, permission, origin) => clipboard(permission) && wc === win?.webContents && origin === ORIGIN);
  session.defaultSession.setDevicePermissionHandler(() => false);
  serveScreen();
  relay();
  const window = {
    focused: () => win?.isFocused() ?? false,
    show,
    parent: () => win,
    answer: (picked: Picked) => host?.note(picked),
    hidden,
  };
  host = Sidecar.start({
    event: (event) => {
      // The host dropped events this screen never heard (it fell too far behind): the screen starts over from a fresh
      // reading instead of drawing from a partial one. Rare; Tauri's direct events never lag.
      if ((event as { type?: unknown } | null)?.type === 'lagged') win?.webContents.reload();
      else win?.webContents.send('faber:event', event);
    },
    order: (order) => apply(order, window),
  });
  if (!autostart) show();
});
