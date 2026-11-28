// Runs only as an Electron main entry; every window stays hidden and all data is synthetic.
import { app, BrowserWindow, protocol, session } from 'electron';
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { stripTypeScriptTypes } from 'node:module';

const [mode, dir] = process.argv.slice(2);
let interrupted = false;
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
      localStorage.setItem('faber.unsent.attachment-only', 'old attachment field');
      localStorage.setItem('faber.unsent.untouched', 'still import this field');
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
    if (mode === 'crash-publication') app.on('web-contents-created', (_event, contents) => {
      const execute = contents.executeJavaScript.bind(contents);
      contents.executeJavaScript = (script, ...args) => execute(script.replace("localStorage.setItem(markerKey, '1');", "await new Promise(() => console.log('CRASH_BEFORE_ATTACHMENT_MARKER'));"), ...args);
      contents.on('console-message', (event) => {
        if (event.message !== 'CRASH_BEFORE_ATTACHMENT_MARKER') return;
        // Stop after the real IDB transaction and text write, without running the importer's rollback.
        session.defaultSession.flushStorageData();
        console.log('PASS: interrupted import before attachment marker publication');
        interrupted = true; clearTimeout(watchdog); app.exit(0);
      });
    });
    if (mode === 'failed-read-idle') app.on('web-contents-created', (_event, contents) => contents.on('did-finish-load', () => {
      if (contents.getURL() === 'http://tauri.localhost/') void contents.executeJavaScript(`(() => {
        const original = Storage.prototype.getItem;
        Storage.prototype.getItem = function(key) {
          if (key === 'faber.unsent.new-chat-') throw new Error('injected legacy read failure');
          return original.call(this, key);
        };
      })()`);
    }));
    if (mode === 'quota' || mode === 'partial' || mode === 'failed-use' || mode === 'failed-idle') {
      const failingKey = mode === 'partial' ? 'faber.tauri-drafts-imported' : 'faber.unsent-attachments.new-chat-';
      app.on('web-contents-created', (_event, contents) => contents.on('did-finish-load', () => {
        // Force the failed field first, so this launch also leaves a known field unvisited.
        if (mode === 'failed-use' && contents.getURL() === 'http://tauri.localhost/') void contents.executeJavaScript(`(() => {
          const original = Object.keys;
          Object.keys = (value) => original(value).sort((a, b) => Number(b.endsWith('.new-chat-')) - Number(a.endsWith('.new-chat-')));
        })()`);
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
    if (['restart-dirty', 'failed-idle', 'failed-read-idle', 'hydrate-pending', 'close-dirty'].includes(mode)) await window.webContents.executeJavaScript(`globalThis.faberShell = {
      flushStorage: () => {}, invoke: async (command, args) => { if (command === 'draft_dirty') globalThis.updaterDirty = args.dirty; }, listen: () => () => {}, window: { isMaximized: async () => false, onResized: () => () => {} },
    }; void 0;`);
    if (mode === 'close-dirty') {
      const main = readFileSync(new URL('./main.ts', import.meta.url), 'utf8');
      const guard = main.slice(main.indexOf("window.webContents.on('will-prevent-unload'"), main.indexOf('  // Closing frees'));
      Function('window', stripTypeScriptTypes(`let closingForQuit = false; ${guard}`))(window);
      await window.webContents.executeJavaScript(readFileSync(path.join(dir, 'screen.js'), 'utf8'));
      await window.webContents.executeJavaScript(`
        window.addEventListener('beforeunload', draftScreen.protectDraftClose);
        globalThis.savedWrite = Storage.prototype.setItem;
        Storage.prototype.setItem = function(key, text) {
          if (key === 'faber.unsent.close-proof') throw new Error('synthetic full disk');
          return savedWrite.call(this, key, text);
        };
        globalThis.leaveComposer = draftScreen.mountComposer('close-proof');
        const field = document.querySelector('textarea');
        field.value = 'Synthetic unsaved words';
        field.dispatchEvent(new Event('input', { bubbles: true }));
        leaveComposer();
      `, true);
      assert.equal(await window.webContents.executeJavaScript('updaterDirty'), true, 'navigation must not tell the unattended updater it may install and quitNow while the only text copy remains in memory');
      const refuseClose = async () => {
        const refused = new Promise((resolve) => window.webContents.once('will-prevent-unload', resolve));
        window.close(); await refused;
        assert.equal(window.isDestroyed(), false, 'a refused close retains the only current draft copy');
      };
      await refuseClose();
      assert.equal(await window.webContents.executeJavaScript("draftScreen.kept('close-proof')"), 'Synthetic unsaved words');
      assert.equal(window.isVisible(), false, 'the refused close hides the protected renderer');
      assert.equal(await window.webContents.executeJavaScript('updaterDirty'), true, 'the Windows unattended updater must still receive draft_dirty=true after navigation and hidden close, preventing its quitNow path');
      await window.webContents.executeJavaScript(`
        Storage.prototype.setItem = savedWrite;
        globalThis.savedOpen = IDBFactory.prototype.open;
        IDBFactory.prototype.open = () => { throw new Error('synthetic attachment failure'); };
        draftScreen.keepAttachments('close-proof', { images: [], files: [{ name: 'synthetic.pdf', base64: 'AAEC' }], inlined: [] });
      `);
      await refuseClose();
      assert.equal(await window.webContents.executeJavaScript("draftScreen.keptAttachments('close-proof').then((draft) => draft.files[0].base64)"), 'AAEC');
      const cpu = () => new Map(app.getAppMetrics().map((metric) => [metric.pid, metric.cpu.cumulativeCPUUsage]));
      const beforeCpu = cpu();
      await new Promise((resolve) => setTimeout(resolve, 500));
      // Startup helpers may exit between samples; compare only the same surviving process identities.
      const cpuSeconds = [...cpu()].reduce((sum, [pid, seconds]) => sum + (beforeCpu.has(pid) ? Math.max(0, seconds - beforeCpu.get(pid)) : 0), 0);
      console.log('COST: synthetic hidden retained draft working_set_kib=' + app.getAppMetrics().reduce((sum, metric) => sum + metric.memory.workingSetSize, 0) + ' surviving_cpu_ms_per_500ms=' + Math.round(cpuSeconds * 1000));
      assert.equal(await window.webContents.executeJavaScript(`(async () => {
        IDBFactory.prototype.open = savedOpen;
        await draftScreen.keepAttachments('close-proof', await draftScreen.keptAttachments('close-proof'));
        return draftScreen.flushDrafts();
      })()`), true);
      assert.equal(await window.webContents.executeJavaScript('updaterDirty'), false, 'successful saves must release the updater admission guard');
      const closed = new Promise((resolve) => window.once('closed', resolve));
      window.close(); await closed;
      console.log('PASS: failed text and attachment saves survive native close, and successful retry releases the renderer');
      clearTimeout(watchdog); app.quit(); return;
    }
    if (mode === 'restart-dirty') {
      await window.webContents.executeJavaScript(readFileSync(path.join(dir, 'screen.js'), 'utf8'));
      await window.webContents.executeJavaScript('draftScreen.restartWithFailedDraft()');
      window.destroy(); console.log('PASS: engine recovery retains newer unsaved words and refreshes state');
      clearTimeout(watchdog); app.quit(); return;
    }
    await window.webContents.executeJavaScript("localStorage.setItem('faber.unsent.existing', 'newer Electron draft')");
    const { migrateDrafts = async () => {} } = await import('../out/electron/migrate.js').catch((error) => {
      if (error.code === 'ERR_MODULE_NOT_FOUND') return {}; throw error;
    });
    if (mode === 'hydrate-pending' || mode === 'failed-read-idle') {
      assert.deepEqual(await window.webContents.executeJavaScript(`[
        localStorage.getItem('faber.tauri-drafts-imported.new-chat-'),
        localStorage.getItem('faber.unsent.new-chat-'), localStorage.getItem('faber.unsent-attachments.new-chat-')
      ]`), ['pending', 'my unsent draft', null], 'interruption must leave committed text without an attachment marker');
      if (mode === 'failed-read-idle') await assert.rejects(migrateDrafts(dir), /injected legacy read failure/);
      await window.webContents.executeJavaScript(readFileSync(path.join(dir, 'screen.js'), 'utf8'));
      await window.webContents.executeJavaScript("void draftScreen.mountComposer('new-chat-')");
      assert.equal(await window.webContents.executeJavaScript("document.querySelector('textarea').value"), '', 'the composer must never expose half-published import text');
      if (mode === 'hydrate-pending') assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.tauri-drafts-imported.new-chat-')"), 'pending', 'hydration must not finalize a pending import');
      if (mode === 'hydrate-pending') assert.equal(await window.webContents.executeJavaScript("draftScreen.keep('new-chat-', 'my unsent draft')"), false, 'pending publication must refuse screen persistence too');
      else assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.tauri-drafts-imported.new-chat-')"), null, 'startup must roll back pending publication before the failed legacy read');
      session.defaultSession.flushStorageData(); window.destroy();
      console.log('PASS: ' + mode + ' with untouched real composer');
      clearTimeout(watchdog); app.quit(); return;
    }
    if (mode === 'failed-idle') {
      await assert.rejects(migrateDrafts(dir), /injected quota failure/);
      assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.new-chat-')"), null, 'failed publication rolls back text');
      await window.webContents.executeJavaScript(readFileSync(path.join(dir, 'screen.js'), 'utf8'));
      await window.webContents.executeJavaScript("void draftScreen.mountComposer('new-chat-')");
      assert.equal(await window.webContents.executeJavaScript("document.querySelector('textarea').value"), '', 'the untouched real composer starts empty');
      session.defaultSession.flushStorageData(); window.destroy();
      console.log('PASS: failed import followed by untouched composer mount');
      clearTimeout(watchdog); app.quit(); return;
    }
    if (mode === 'failed-use') {
      await assert.rejects(migrateDrafts(dir), /injected quota failure/);
      assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.tauri-drafts-imported.text-only')"), null, 'the loop has not reached this field');
      // Run the real screen persistence code with only reactivity and the shell notification stubbed.
      const source = stripTypeScriptTypes(readFileSync(new URL('../ui/unsent.ts', import.meta.url), 'utf8')).replace(/^import .+;\r?$/gm, '').replace(/^export /gm, '');
      await window.webContents.executeJavaScript(`(async () => {
        const createSignal = (initial) => { let value = initial; return [() => value, (next) => { value = typeof next === 'function' ? next(value) : next; }]; };
        const api = { draftDirty: async () => {} };
        let flushes = 0;
        const flushStorage = () => { flushes++; };
        ${source}
        for (const key of ['new-chat-', 'text-only']) {
          if (!keep(key, 'new message sent in Electron') || !keep(key, '')) throw new Error('screen draft write failed');
          if (localStorage.getItem('faber.unsent.' + key) !== null || localStorage.getItem('faber.unsent-attachments.' + key) !== null) throw new Error('consumed field must have no draft keys');
        }
        if (flushes !== 4) throw new Error('each text write and consumption must request a flush');
        if (!await keepAttachments('attachment-only', { images: [], files: [{ name: 'new.txt', base64: 'bmV3' }], inlined: [] })) throw new Error('screen attachment write failed');
        if (flushes !== 5) throw new Error('attachment marker publication must request a flush');
        if (!await keepAttachments('attachment-only', { images: [], files: [], inlined: [] })) throw new Error('screen attachment consumption failed');
        if (flushes !== 7) throw new Error('attachment removal intent and completion must each request a flush');
        if (localStorage.getItem('faber.unsent-attachments.attachment-only') !== null) throw new Error('consumed attachment marker must be gone');
        localStorage.setItem('faber.unsent.blocked-text', 'retain this draft');
        const original = Storage.prototype.setItem;
        Storage.prototype.setItem = function(key, value) {
          if (key === 'faber.tauri-drafts-imported.blocked-text' || key === 'faber.tauri-drafts-imported.blocked-attachment') throw new Error('injected progress failure');
          return original.call(this, key, value);
        };
        try {
          if (keep('blocked-text', '') || kept('blocked-text') !== '' || dirtyCount() !== 1) throw new Error('failed progress must retain deletion intent and block unattended update');
          if (localStorage.getItem('faber.unsent.blocked-text') !== 'retain this draft') throw new Error('failed progress must not consume durable text');
          if (await keepAttachments('blocked-attachment', { images: [], files: [{ name: 'new.txt', base64: 'bmV3' }], inlined: [] })) throw new Error('failed progress must report attachment failure');
          if (await keepAttachments('blocked-attachment', { images: [], files: [], inlined: [] })) throw new Error('failed progress must protect attachment consumption too');
        } finally { Storage.prototype.setItem = original; }
        if (flushes !== 7) throw new Error('failed progress must not publish or flush a draft mutation');
        if (!keep('blocked-text', '') || !await keepAttachments('blocked-attachment', { images: [], files: [], inlined: [] }) || dirtyCount() !== 0) throw new Error('retry must release failed-write protection');
      })()`);
      session.defaultSession.flushStorageData(); window.destroy();
      console.log('PASS: failed import followed by screen writes and consumption, including an unreached field');
      clearTimeout(watchdog); app.quit(); return;
    }
    if (mode === 'retry-used') {
      await migrateDrafts(dir);
      const consumed = await window.webContents.executeJavaScript(`['new-chat-', 'text-only', 'attachment-only'].map((key) => [key, localStorage.getItem('faber.unsent.' + key), localStorage.getItem('faber.unsent-attachments.' + key)])`);
      assert.deepEqual(consumed, [['new-chat-', null, null], ['text-only', null, null], ['attachment-only', null, null]], 'retry must not resurrect failed or unreached fields consumed by the screen');
      assert.equal(await window.webContents.executeJavaScript("localStorage.getItem('faber.unsent.untouched')"), 'still import this field', 'unused legacy fields must still import');
      window.destroy(); console.log('PASS: retry preserves consumed failed and unreached fields');
      clearTimeout(watchdog); app.quit(); return;
    }
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
} catch (error) { if (!interrupted) { console.error(error); clearTimeout(watchdog); app.exit(1); } } });
