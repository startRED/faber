// Runs only as an Electron main entry; every window stays hidden and all data is synthetic.
import { app, BrowserWindow, protocol, session } from 'electron';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import path from 'node:path';

const [mode, dir] = process.argv.slice(2);
app.setPath('userData', path.join(dir, 'webview'));
app.disableHardwareAcceleration();
app.enableSandbox();
protocol.registerSchemesAsPrivileged([{ scheme: 'faber', privileges: { standard: true, secure: true, supportFetchAPI: true } }]);
app.on('window-all-closed', () => {});
const watchdog = setTimeout(() => { console.error('migration proof timed out'); app.exit(1); }, 30_000);
const blank = () => new Response('', { headers: { 'Content-Type': 'text/html', 'Content-Security-Policy': "default-src 'none'; script-src 'none'" } });
void app.whenReady().then(async () => { try {
  if (mode === 'seed') {
    const source = path.join(dir, 'webview', 'EBWebView', 'Default');
    mkdirSync(source, { recursive: true });
    const ses = session.fromPath(source);
    ses.protocol.handle('http', blank);
    const window = new BrowserWindow({ show: false, webPreferences: { session: ses, sandbox: true, nodeIntegration: false, contextIsolation: true } });
    await window.loadURL('http://tauri.localhost');
    await window.webContents.executeJavaScript(`(async () => {
      localStorage.setItem('faber.unsent.new-chat-', 'my unsent draft');
      localStorage.setItem('faber.unsent-attachments.new-chat-', '1');
      localStorage.setItem('faber.unsent.existing', 'old words');
      localStorage.setItem('faber.unsent-attachments.existing', '1');
      localStorage.setItem('faber.unsent.deleted', 'must stay gone');
      localStorage.setItem('faber.unsent-attachments.deleted', 'forget');
      localStorage.setItem('faber.unsent.text-only', 'only text');
      localStorage.setItem('unrelated', 'never import');
      const db = await new Promise((resolve, reject) => {
        const request = indexedDB.open('faber.unsent', 1);
        request.onupgradeneeded = () => request.result.createObjectStore('attachments');
        request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error);
      });
      await new Promise((resolve, reject) => {
        const tx = db.transaction('attachments', 'readwrite', { durability: 'strict' });
        tx.objectStore('attachments').put({ images: [{ name: 'image.png', mediaType: 'image/png', bytes: new Blob([new Uint8Array([0, 1, 255])], { type: 'image/png' }) }], files: [{ name: 'file.txt', bytes: new Blob(['file bytes']) }], inlined: [{ name: 'inline.txt', block: 'inline text' }] }, 'new-chat-');
        tx.oncomplete = resolve; tx.onerror = tx.onabort = () => reject(tx.error);
      }); db.close();
    })()`);
    ses.flushStorageData();
    window.destroy();
    console.log('seeded legacy profile', ses.storagePath);
  } else {
    protocol.handle('faber', blank);
    if (mode === 'quota' || mode === 'partial') {
      const failingKey = mode === 'quota' ? 'faber.unsent-attachments.new-chat-' : 'faber.tauri-drafts-imported';
      app.on('web-contents-created', (_event, contents) => contents.on('did-finish-load', () => {
        if (contents.getURL() === 'faber://app/__draft_import__') void contents.executeJavaScript(`(() => {
          const original = Storage.prototype.setItem;
          Storage.prototype.setItem = function(key, value) {
            if (key === ${JSON.stringify(failingKey)}) throw new Error('injected quota failure');
            return original.call(this, key, value);
          };
        })()`);
      }));
    }
    const window = new BrowserWindow({ show: false, webPreferences: { sandbox: true, nodeIntegration: false, contextIsolation: true } });
    await window.loadURL('faber://app/');
    await window.webContents.executeJavaScript("localStorage.setItem('faber.unsent.existing', 'newer Electron draft')");
    const { migrateDrafts = async () => {} } = await import('../out/electron/migrate.js').catch((error) => {
      if (error.code === 'ERR_MODULE_NOT_FOUND') return {}; throw error;
    });
    if (mode === 'quota' || mode === 'partial') {
      await assert.rejects(migrateDrafts(dir), /injected quota failure/);
      if (mode === 'quota') {
        assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.new-chat-')"), null, 'failed attachment publication must roll back partial text');
      } else {
        assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.new-chat-')"), 'my unsent draft');
        await window.webContents.executeJavaScript("localStorage.removeItem('faber.unsent.new-chat-'); localStorage.removeItem('faber.unsent-attachments.new-chat-')");
      }
      session.defaultSession.flushStorageData(); window.destroy();
      console.log('PASS: injected ' + mode + ' import failure');
      clearTimeout(watchdog); app.quit(); return;
    }
    if (mode === 'retry') {
      await migrateDrafts(dir);
      assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.new-chat-')"), null, 'retry must not resurrect a consumed field from a partial import');
      window.destroy(); console.log('PASS: partial import retry preserves consumption');
      clearTimeout(watchdog); app.quit(); return;
    }
    const started = performance.now();
    const cpuBefore = process.cpuUsage();
    await migrateDrafts(dir);
    const cpu = process.cpuUsage(cpuBefore);
    console.log('COST: synthetic import wall_ms=' + Math.round(performance.now() - started) + ' main_cpu_ms=' + Math.round((cpu.user + cpu.system) / 1000) + ' working_set_kib=' + app.getAppMetrics().reduce((sum, metric) => sum + metric.memory.workingSetSize, 0));
    assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.new-chat-')"), 'my unsent draft', 'Tauri draft must reach the new origin');
    assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent-attachments.new-chat-')"), '1');
    assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.existing')"), 'newer Electron draft');
    assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.deleted')"), null);
    assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('unrelated')"), null);
    const bytes = await window.webContents.executeJavaScript(`(async () => {
      const db = await new Promise((resolve, reject) => { const r = indexedDB.open('faber.unsent', 1); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error); });
      const value = await new Promise((resolve, reject) => { const tx = db.transaction('attachments'); const r = tx.objectStore('attachments').get('new-chat-'); tx.oncomplete = () => resolve(r.result); tx.onerror = () => reject(tx.error); });
      const answer = { image: [...new Uint8Array(await value.images[0].bytes.arrayBuffer())], imageType: value.images[0].bytes.type, file: await value.files[0].bytes.text(), inlined: value.inlined };
      db.close(); return answer;
    })()`);
    assert.deepEqual(bytes, { image: [0, 1, 255], imageType: 'image/png', file: 'file bytes', inlined: [{ name: 'inline.txt', block: 'inline text' }] });
    await window.webContents.executeJavaScript("localStorage.removeItem('faber.unsent.new-chat-'); localStorage.removeItem('faber.unsent-attachments.new-chat-')");
    await migrateDrafts(dir);
    assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.new-chat-')"), null, 'a later launch must not resurrect a cleared imported draft');
    window.destroy();
    console.log('PASS: legacy text, binary attachments, conflict and deletion preservation, idempotence');
  }
  clearTimeout(watchdog); app.quit();
} catch (error) { console.error(error); clearTimeout(watchdog); app.exit(1); } });
