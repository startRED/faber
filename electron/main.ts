// Faber's Electron shell (R459): one window, the screen from dist/, and a relay to the Rust host. Nothing here decides
// anything a person sees; the commands, their rules and the events all live in Rust (`host/`). This file only creates
// the window, serves the screen, and carries calls across.

import { app, BrowserWindow, ipcMain, type IpcMainEvent, type IpcMainInvokeEvent, net, protocol, session, shell } from 'electron';
import { createHash } from 'node:crypto';
import { readFileSync, realpathSync, statSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { Sidecar } from './sidecar.ts';

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
// The real folder, so a link inside it can never lead the screen's scheme outside.
const dist = realpathSync(path.join(root, 'dist'));
const hidden = process.env.FABER_HIDDEN === '1';
const autostart = process.argv.includes('--autostart');

// Every renderer runs in the system sandbox, not only the ones that ask for it.
app.enableSandbox();
// The screen loads only from Faber's own scheme: never file:// and never a remote page.
protocol.registerSchemesAsPrivileged([{ scheme: SCHEME, privileges: { standard: true, secure: true, supportFetchAPI: true } }]);

// Features Faber does not use stay off; each one costs memory in every renderer.
app.commandLine.appendSwitch('disable-features', 'SpareRendererForSitePerProcess,MediaRouter,Translate');
app.commandLine.appendSwitch('disable-spell-checking');

if (!app.requestSingleInstanceLock()) app.quit();

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
  // Closing frees the screen (its renderer and its memory); the host keeps running answers in the background.
  window.on('closed', () => {
    if (win === window) win = null;
    host?.call('quota_attention', { visible: false, focused: false }).catch(() => {});
  });
  void window.loadURL(`${ORIGIN}/index.html`);
  return window;
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
    if (!fromScreen(event)) throw new Error('error.host.notScreen');
    if (typeof command !== 'string') throw new Error('error.host.badRequest');
    if (args !== undefined && (args === null || typeof args !== 'object' || Array.isArray(args))) throw new Error('error.host.badArgs');
    const given = (args ?? {}) as Record<string, unknown>;
    const own = windowCommands.get(command);
    if (own) return own(given);
    if (!host) throw new Error('error.host.notReady');
    return host.call(command, given);
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

// Faber stays in the tray when its window closes; only Quit ends it (the tray arrives in phase 4).
app.on('window-all-closed', () => {});

// Quitting waits for the host to save what it holds and stop; one that hangs is ended after a few seconds.
let stopping = false;
app.on('before-quit', (event) => {
  if (stopping || !host) return;
  stopping = true;
  event.preventDefault();
  void host.stop(HOST_STOP_MS).finally(() => app.quit());
});

void app.whenReady().then(() => {
  // The screen asks for nothing a browser grants: no camera, no notifications from the page, no geolocation, no devices.
  session.defaultSession.setPermissionRequestHandler((_wc, _permission, callback) => callback(false));
  session.defaultSession.setPermissionCheckHandler(() => false);
  session.defaultSession.setDevicePermissionHandler(() => false);
  serveScreen();
  relay();
  host = Sidecar.start((event) => {
    // The host dropped events this screen never heard (it fell too far behind): the screen starts over from a fresh
    // reading instead of drawing from a partial one. Rare; Tauri's direct events never lag.
    if ((event as { type?: unknown } | null)?.type === 'lagged') win?.webContents.reload();
    else win?.webContents.send('faber:event', event);
  });
  if (!autostart) show();
});
