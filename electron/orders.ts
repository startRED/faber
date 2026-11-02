// What the host decides and the shell only carries out (`Order` in `host/src/wire.rs`): the tray icon and its menu,
// notices, keeping the PC awake, starting with the system, the file dialogs and showing a file in the file manager. Every order says what is true now, so applying one twice
// does no harm; the decisions (the setting, a game in front, one notice per question) were all made in Rust.

import { app, type BrowserWindow, dialog, Menu, Notification, nativeImage, powerSaveBlocker, shell, Tray } from 'electron';

type TrayOrder = {
  kind: 'tray';
  tooltip: string;
  needsYou: number;
  picture: string;
  picture16: string;
  menu: { show: string; quit: string };
};
type NotifyOrder = { kind: 'notify'; title: string; body: string; conversationId: string | null; toast: boolean; sound: boolean };
type PickOrder = {
  kind: 'pick';
  id: number;
  mode: 'folder' | 'file' | 'save';
  filter: { name: string; extensions: string[] } | null;
  name: string | null;
};
type Order =
  | TrayOrder
  | NotifyOrder
  | PickOrder
  | { kind: 'awake'; on: boolean }
  | { kind: 'autostart'; on: boolean }
  | { kind: 'reveal'; path: string }
  | { kind: 'quit' };

/** The answer to a dialog, as the host reads it (`ShellNote::Picked`). */
export type Picked = { kind: 'picked'; id: number; path: string | null; noWindow: boolean };

/** What the orders need from the window, which the shell owns. */
export type Window = {
  /** True while the person is looking at Faber: a notice then would only interrupt. */
  focused: () => boolean;
  /** Brings the window back (from the tray, or after a notice is clicked). */
  show: () => void;
  /** The window a dialog belongs to, if one is open. */
  parent: () => BrowserWindow | null;
  /** Gives the host a dialog's answer. */
  answer: (picked: Picked) => void;
  /** A test run shows nothing outside its window and changes nothing on the system. */
  hidden: boolean;
};

const KINDS = new Set(['tray', 'notify', 'awake', 'autostart', 'pick', 'reveal', 'quit']);

let tray: Tray | null = null;
let blocker: number | null = null;
// A notice the system still shows is kept here: one collected by the garbage collector loses its click.
const shown = new Set<Notification>();

function isOrder(value: unknown): value is Order {
  const kind = (value as { kind?: unknown } | null)?.kind;
  return typeof kind === 'string' && KINDS.has(kind);
}

/** The tray picture from both sizes, so the system picks the one that fits the display instead of scaling one up. */
function picture(order: TrayOrder): Electron.NativeImage {
  const image = nativeImage.createFromBuffer(Buffer.from(order.picture16, 'base64'), { scaleFactor: 1 });
  image.addRepresentation({ scaleFactor: 2, buffer: Buffer.from(order.picture, 'base64') });
  return image;
}

function showTray(order: TrayOrder, window: Window): void {
  if (window.hidden) return;
  if (!tray) {
    tray = new Tray(picture(order));
    tray.on('click', window.show);
  } else tray.setImage(picture(order));
  tray.setToolTip(order.tooltip);
  tray.setContextMenu(
    Menu.buildFromTemplate([
      { label: order.menu.show, click: window.show },
      { type: 'separator' },
      { label: order.menu.quit, click: () => app.quit() },
    ]),
  );
}

function notify(order: NotifyOrder, window: Window): void {
  // The host plays the chime itself; the notice is always silent so the system's sound never doubles it.
  if (!order.toast || window.hidden || window.focused() || !Notification.isSupported()) return;
  const notice = new Notification({ title: order.title, body: order.body, silent: true });
  shown.add(notice);
  const done = () => shown.delete(notice);
  notice.on('click', () => {
    done();
    window.show();
  });
  notice.on('close', done);
  notice.on('failed', done);
  notice.show();
}

function keepAwake(on: boolean): void {
  if (on && blocker === null) blocker = powerSaveBlocker.start('prevent-app-suspension');
  else if (!on && blocker !== null) {
    powerSaveBlocker.stop(blocker);
    blocker = null;
  }
}

/**
 * The name of Faber's value in the Windows startup list. The Tauri Faber wrote "Faber" with the same command
 * (`"<faber.exe>" --autostart`), so an install that moved to Electron keeps reading and changing that one, never a second.
 */
export const LOGIN_ITEM = 'Faber';

function startWithSystem(on: boolean, window: Window): void {
  // Only the installed app registers itself: a copy run from source would put a developer's Electron in the startup list.
  if (window.hidden || !app.isPackaged) return;
  app.setLoginItemSettings({ openAtLogin: on, args: on ? ['--autostart'] : [], name: LOGIN_ITEM });
}

/** Asks the person for a folder, a file to open or where to save one, in a dialog that belongs to the window. */
async function pick(order: PickOrder, window: Window): Promise<void> {
  const parent = window.parent();
  if (!parent || window.hidden) {
    window.answer({ kind: 'picked', id: order.id, path: null, noWindow: true });
    return;
  }
  const filters = order.filter ? [{ name: order.filter.name, extensions: order.filter.extensions }] : [];
  let path: string | null = null;
  try {
    if (order.mode === 'save') {
      const saved = await dialog.showSaveDialog(parent, { defaultPath: order.name ?? undefined, filters });
      path = saved.canceled ? null : (saved.filePath ?? null);
    } else {
      const properties: ('openDirectory' | 'createDirectory' | 'openFile')[] =
        order.mode === 'folder' ? ['openDirectory', 'createDirectory'] : ['openFile'];
      const opened = await dialog.showOpenDialog(parent, { properties, filters });
      path = opened.canceled ? null : (opened.filePaths[0] ?? null);
    }
  } catch {
    // A dialog that could not run counts as cancelled, as it does in the Tauri shell.
  }
  window.answer({ kind: 'picked', id: order.id, path, noWindow: false });
}

/** Carries out one order from the host; anything that is not an order is ignored. */
export function apply(value: unknown, window: Window): void {
  if (!isOrder(value)) return;
  switch (value.kind) {
    case 'tray':
      showTray(value, window);
      break;
    case 'notify':
      notify(value, window);
      break;
    case 'awake':
      keepAwake(value.on);
      break;
    case 'autostart':
      startWithSystem(value.on, window);
      break;
    case 'pick':
      void pick(value, window);
      break;
    case 'reveal':
      if (!window.hidden) shell.showItemInFolder(value.path);
      break;
    case 'quit':
      // The self-update is about to replace this program: it ends now, not after the usual wait for the host to stop
      // (the host sent this order itself, and a copy of it finishes the install).
      release();
      app.exit(0);
      break;
  }
}

/** Lets the PC sleep and removes the icon, as the app quits. */
export function release(): void {
  keepAwake(false);
  tray?.destroy();
  tray = null;
}
