// Sandboxed preloads must be CommonJS. This is the only bridge between the page and the engine.
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('faber', {
  call: (method: string, arg?: unknown) => ipcRenderer.invoke('faber', method, arg),
  on: (listener: (event: unknown) => void) => {
    const handler = (_event: unknown, payload: unknown) => listener(payload);
    ipcRenderer.on('faber:event', handler);
    return () => ipcRenderer.off('faber:event', handler);
  },
});
