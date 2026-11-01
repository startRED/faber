import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

// The one place the screen reaches the shell that draws it (R459): Tauri until the move, Electron after. Electron's
// preload (`electron/preload.cts`) puts `faberShell` on the window; with it absent, the screen runs in Tauri.

type Unlisten = () => void;

/** What Electron's preload exposes: the same command list, the same events, the window's buttons. */
interface ElectronShell {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  listen(event: string, listener: (payload: unknown) => void): Unlisten;
  window: {
    minimize(): void;
    toggleMaximize(): void;
    close(): void;
    isMaximized(): Promise<boolean>;
    onResized(listener: () => void): Unlisten;
  };
}

const electron = (globalThis as { faberShell?: ElectronShell }).faberShell;

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return electron ? (electron.invoke(command, args) as Promise<T>) : tauriInvoke<T>(command, args);
}

/** Tauri's shape, `{ payload }`, so every listener in `api.ts` stays as it is. */
export function listen<T>(event: string, listener: (event: { payload: T }) => void): Promise<Unlisten> {
  if (electron) return Promise.resolve(electron.listen(event, (payload) => listener({ payload: payload as T })));
  return tauriListen<T>(event, listener);
}

/** The window's buttons: minimize, maximize or restore, close, and whether it is maximized. */
export interface ShellWindow {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  onResized(listener: () => void): Promise<Unlisten>;
}

export function currentWindow(): ShellWindow {
  if (!electron) return getCurrentWindow();
  const win = electron.window;
  return {
    minimize: async () => win.minimize(),
    toggleMaximize: async () => win.toggleMaximize(),
    close: async () => win.close(),
    isMaximized: () => win.isMaximized(),
    onResized: async (listener) => win.onResized(listener),
  };
}
