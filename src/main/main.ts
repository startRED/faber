import { app, BrowserWindow, dialog, shell } from 'electron';
import electronUpdater from 'electron-updater';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { updateMode } from './update-policy.ts';

const { autoUpdater } = electronUpdater;
const here = dirname(fileURLToPath(import.meta.url));
const RELEASES_URL = 'https://github.com/startRED/faber/releases/latest';

function createWindow(): void {
  const win = new BrowserWindow({
    width: 1280,
    height: 800,
    title: 'Faber',
    backgroundColor: '#111216',
    webPreferences: { sandbox: true, contextIsolation: true },
  });
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
  createWindow();
  checkForUpdates();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

app.on('activate', () => {
  if (BrowserWindow.getAllWindows().length === 0) createWindow();
});
