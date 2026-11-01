// What the screen may reach of the shell (R459): the command list, the events, the window's buttons. Nothing else
// crosses: no Node, no ipcRenderer, no file access. `ui/shell.ts` reads it as `faberShell`.

// A sandboxed preload is CommonJS: `require` is the only way in.
const { contextBridge, ipcRenderer } = require('electron') as typeof import('electron');
type IpcRendererEvent = import('electron').IpcRendererEvent;

/** The events `ui/api.ts` listens to, as the Tauri shell names them. */
const EVENTS = new Set([
  'faber:event',
  'faber:update',
  'faber:site',
  'faber:site-loading',
  'faber:site-key',
  'faber:site-tabs',
  'faber:site-busy',
]);

contextBridge.exposeInMainWorld('faberShell', {
  invoke: async (command: string, args?: Record<string, unknown>) => {
    // A failure is thrown as the host gave it (a key, or `{key, detail}`), as Tauri's `invoke` does.
    const answer = (await ipcRenderer.invoke('faber:invoke', command, args ?? {})) as { ok?: unknown; err?: unknown };
    if ('err' in answer) throw answer.err;
    return answer.ok;
  },
  listen: (channel: string, listener: (payload: unknown) => void) => {
    // Only the events the shell sends the screen; any other name hears nothing.
    if (!EVENTS.has(channel)) return () => {};
    const handler = (_e: IpcRendererEvent, payload: unknown) => listener(payload);
    ipcRenderer.on(channel, handler);
    return () => void ipcRenderer.removeListener(channel, handler);
  },
  window: {
    minimize: () => ipcRenderer.send('faber:window', 'minimize'),
    toggleMaximize: () => ipcRenderer.send('faber:window', 'toggleMaximize'),
    close: () => ipcRenderer.send('faber:window', 'close'),
    isMaximized: () => ipcRenderer.invoke('faber:window-maximized') as Promise<boolean>,
    onResized: (listener: () => void) => {
      const handler = () => listener();
      ipcRenderer.on('faber:window-resized', handler);
      return () => void ipcRenderer.removeListener('faber:window-resized', handler);
    },
  },
});
