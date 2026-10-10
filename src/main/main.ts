import { app, BrowserWindow, dialog, ipcMain, shell } from 'electron';
import electronUpdater from 'electron-updater';
import { copyFile, readFile, stat } from 'node:fs/promises';
import { basename, dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createEngine, type AppEvent, type Engine } from '../engine/engine.ts';
import { resolveCli, type Cli } from '../engine/resolve-cli.ts';
import type { Vendor } from '../engine/store.ts';
import { updateMode } from './update-policy.ts';

const { autoUpdater } = electronUpdater;
const here = dirname(fileURLToPath(import.meta.url));
const RELEASES_URL = 'https://github.com/startRED/faber/releases/latest';
const IMAGE_TYPES: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.svg': 'image/svg+xml',
};

const hidden = process.env.FABER_HIDDEN === '1';
// A separate data folder (tests, judges) also gets its own browser profile, so nothing leaks between runs.
if (process.env.FABER_DATA_DIR) app.setPath('userData', process.env.FABER_DATA_DIR);
let win: BrowserWindow | undefined;
let engine: Engine;

// Each CLI is found on PATH. FABER_CLI_<NAME> (a JSON Cli) replaces it, so end-to-end tests can run a recorded
// session through a fake CLI instead of a real subscription.
function detectClis(): Partial<Record<Vendor, Cli>> {
  const clis: Partial<Record<Vendor, Cli>> = {};
  for (const vendor of ['claude', 'codex'] as const) {
    const override = process.env[`FABER_CLI_${vendor.toUpperCase()}`];
    const cli = override !== undefined ? (override ? (JSON.parse(override) as Cli) : undefined) : resolveCli(vendor);
    if (cli) clis[vendor] = cli;
  }
  return clis;
}

// The UI calls the engine through one channel with a fixed list of methods; nothing else crosses over.
async function call(method: string, arg: any): Promise<unknown> {
  switch (method) {
    case 'state':
      return engine.state();
    case 'open':
      return engine.open(arg);
    case 'send':
      return engine.send(arg);
    case 'warm':
      return engine.warm(arg);
    case 'interrupt':
      return engine.interrupt(arg);
    case 'answer':
      return engine.answer(arg.conversationId, arg.requestId, arg.allow, arg.always === true);
    case 'rename':
      return engine.rename(arg.id, arg.title);
    case 'remove':
      return engine.remove(arg);
    case 'removeProject':
      return engine.removeProject(arg);
    case 'saveFile': {
      const source = engine.workspaceFile(arg.conversationId, arg.path);
      if (!source || !win) return false;
      const target = await dialog.showSaveDialog(win, { defaultPath: basename(source) });
      if (target.canceled || !target.filePath) return false;
      await copyFile(source, target.filePath);
      return true;
    }
    case 'showFile': {
      const source = engine.workspaceFile(arg.conversationId, arg.path);
      if (source) shell.showItemInFolder(source);
      return undefined;
    }
    case 'readImage': {
      const source = engine.workspaceFile(arg.conversationId, arg.path);
      const type = source && IMAGE_TYPES[extname(source).toLowerCase()];
      if (!source || !type || (await stat(source)).size > 8_000_000) return undefined;
      return `data:${type};base64,${(await readFile(source)).toString('base64')}`;
    }
    case 'addProject': {
      const picked = win ? await dialog.showOpenDialog(win, { properties: ['openDirectory'] }) : undefined;
      return picked && !picked.canceled && picked.filePaths[0] ? engine.addProject(picked.filePaths[0]) : undefined;
    }
    default:
      throw new Error(`Unknown call: ${method}`);
  }
}

function createWindow(): void {
  win = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 720,
    minHeight: 480,
    title: 'Faber',
    backgroundColor: '#15171b',
      // Automated runs (tests, judges) render offscreen so they never steal the user's screen, at full speed:
    // Chromium otherwise slows timers in hidden windows to once a second and skews every measurement.
    show: !hidden,
    webPreferences: { sandbox: true, contextIsolation: true, backgroundThrottling: !hidden, preload: join(here, '../preload/preload.cjs') },
  });
  // Links in answers open in the user's browser, never inside Faber.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:\/\//.test(url)) void shell.openExternal(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (event) => event.preventDefault());
  void win.loadFile(join(here, '../renderer/index.html'), { query: { version: app.getVersion() } });
}

function checkForUpdates(): void {
  if (!app.isPackaged) return;
  const mode = updateMode(process.platform);
  autoUpdater.autoDownload = mode === 'install';
  if (mode === 'notify') {
    autoUpdater.on('update-available', async (info) => {
      const { response } = await dialog.showMessageBox({
        message: `Faber ${info.version} is available.`,
        buttons: ['Download', 'Later'],
      });
      if (response === 0) await shell.openExternal(RELEASES_URL);
    });
  }
  // In install mode the update downloads in the background and installs when the app quits.
  autoUpdater.checkForUpdatesAndNotify().catch((err: unknown) => console.error('update check failed', err));
}

void app.whenReady().then(() => {
  engine = createEngine({ dataDir: app.getPath('userData'), clis: detectClis() }, (event: AppEvent) => win?.webContents.send('faber:event', event));
  ipcMain.handle('faber', (_event, method: string, arg: unknown) => call(method, arg));
  createWindow();
  checkForUpdates();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

app.on('before-quit', () => engine?.shutdown());

app.on('activate', () => {
  if (BrowserWindow.getAllWindows().length === 0) createWindow();
});
