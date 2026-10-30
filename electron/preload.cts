// What the screen may reach of the shell (R459): the command list, the events, the window's buttons. Nothing else
// crosses: no Node, no ipcRenderer, no file access. `ui/shell.ts` reads it as `faberShell`.

// A sandboxed preload is CommonJS: `require` is the only way in.
const { contextBridge, ipcRenderer } = require('electron') as typeof import('electron');
type IpcRendererEvent = import('electron').IpcRendererEvent;

contextBridge.exposeInMainWorld('faberShell', {
  invoke: async (command: string, args?: Record<string, unknown>) => {
    try {
      return await ipcRenderer.invoke('faber:invoke', command, args ?? {});
    } catch (error) {
      // Electron wraps a thrown error's message in its own sentence; the screen expects the bare key, as Tauri gives it.
      const message = error instanceof Error ? error.message : String(error);
      throw message.replace(/^Error invoking remote method 'faber:invoke': (Error: )?/, '');
    }
  },
  listen: (event: string, listener: (payload: unknown) => void) => {
    const channel = event === 'faber:event' ? 'faber:event' : `faber:${event.replace(/^faber:/, '')}`;
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
