// Faber's Electron shell (R459): one window, the screen from dist/, and a relay to the Rust host. Nothing here decides
// anything a person sees; the commands, their rules and the events all live in Rust (`host/`). This file only creates
// the window, serves the screen, and carries calls across.

import { app, BrowserWindow, ipcMain, net, protocol, session, shell } from 'electron';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { Sidecar } from './sidecar.ts';

const SCHEME = 'faber';
const ORIGIN = `${SCHEME}://app`;
/** The screen's colour before it paints, Dark Room's frame (`ui/styles.css`), so the window never flashes white. */
const FRAME = '#101012';
/** If the screen never says it painted, the window still shows after this long (as the Tauri shell does). */
const REVEAL_FAILSAFE_MS = 3000;

// Compiled into out/electron/, two folders below the repository.
const root = path.resolve(import.meta.dirname, '..', '..');
const dist = path.join(root, 'dist');
const hidden = process.env.FABER_HIDDEN === '1';
const autostart = process.argv.includes('--autostart');

// The screen loads only from Faber's own scheme: never file:// and never a remote page.
protocol.registerSchemesAsPrivileged([{ scheme: SCHEME, privileges: { standard: true, secure: true, supportFetchAPI: true } }]);

// Features Faber does not use stay off; each one costs memory in every renderer.
app.commandLine.appendSwitch('disable-features', 'SpareRendererForSitePerProcess,MediaRouter,Translate');
app.commandLine.appendSwitch('disable-spell-checking');

if (!app.requestSingleInstanceLock()) app.quit();

let win: BrowserWindow | null = null;
let host: Sidecar | null = null;

/** Serves dist/ under faber://app/, refusing any path that leaves it. */
function serveScreen(): void {
  protocol.handle(SCHEME, async (request) => {
    const url = new URL(request.url);
    if (url.host !== 'app') return new Response(null, { status: 404 });
    const relative = decodeURIComponent(url.pathname === '/' ? '/index.html' : url.pathname);
    const file = path.normalize(path.join(dist, relative));
    if (!file.startsWith(dist + path.sep)) return new Response(null, { status: 403 });
    const response = await net.fetch(pathToFileURL(file).toString());
    const headers = new Headers(response.headers);
    if (relative.endsWith('.html')) headers.set('Content-Security-Policy', CSP);
    return new Response(response.body, { status: response.status, headers });
  });
}

/** The same policy the Tauri shell gives the screen, with its own origin in place of Tauri's IPC. */
const CSP = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "font-src 'self' data:",
  'media-src data: blob:',
  "frame-src 'none'",
  "object-src 'none'",
  "base-uri 'none'",
].join('; ');

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
      spellcheck: false,
      // A hidden window keeps throttling its timers, as the system webview did.
      backgroundThrottling: true,
    },
  });
  // A link opens outside the app; the screen itself never navigates away from its origin.
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:\/\//.test(url)) void shell.openExternal(url);
    return { action: 'deny' };
  });
  window.webContents.on('will-navigate', (event, url) => {
    if (!url.startsWith(`${ORIGIN}/`)) event.preventDefault();
  });
  const failsafe = setTimeout(() => reveal(window), REVEAL_FAILSAFE_MS);
  window.once('show', () => clearTimeout(failsafe));
  window.on('maximize', () => window.webContents.send('faber:window-resized'));
  window.on('unmaximize', () => window.webContents.send('faber:window-resized'));
  window.on('resize', () => window.webContents.send('faber:window-resized'));
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

/** Commands only a window can answer. The rest go to the host as they are. */
const windowCommands: Record<string, (args: Record<string, unknown>) => unknown> = {
  screen_ready: () => {
    if (win) reveal(win);
  },
};

function relay(): void {
  ipcMain.handle('faber:invoke', async (event, command: unknown, args: unknown) => {
    // Only the screen calls Faber: any other page (a future browser pane) is refused before a command runs.
    if (!win || event.sender !== win.webContents || event.senderFrame?.url?.startsWith(ORIGIN) !== true) {
      throw new Error('Faber answers only its own screen');
    }
    if (typeof command !== 'string') throw new Error('error.host.badRequest');
    const given = (args && typeof args === 'object' ? args : {}) as Record<string, unknown>;
    const own = windowCommands[command];
    if (own) return own(given);
    if (!host) throw new Error('error.host.notReady');
    return host.call(command, given);
  });
  ipcMain.on('faber:window', (event, action: unknown) => {
    if (!win || event.sender !== win.webContents) return;
    if (action === 'minimize') win.minimize();
    else if (action === 'toggleMaximize') {
      if (win.isMaximized()) win.unmaximize();
      else win.maximize();
    } else if (action === 'close') win.close();
  });
  ipcMain.handle('faber:window-maximized', (event) => (win && event.sender === win.webContents ? win.isMaximized() : false));
}

app.on('second-instance', (_event, argv) => {
  if (!argv.includes('--autostart')) show();
});

// Faber stays in the tray when its window closes; only Quit ends it (the tray arrives in phase 4).
app.on('window-all-closed', () => {});

app.on('before-quit', () => {
  void host?.stop();
});

void app.whenReady().then(async () => {
  // The screen asks for nothing a browser grants: no camera, no notifications from the page, no geolocation.
  session.defaultSession.setPermissionRequestHandler((_wc, _permission, callback) => callback(false));
  serveScreen();
  relay();
  host = Sidecar.start((event) => win?.webContents.send('faber:event', event));
  if (!autostart) show();
});
