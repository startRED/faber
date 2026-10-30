// What the host decides and the shell only carries out (`Order` in `host/src/wire.rs`): the tray icon and its menu,
// notices, keeping the PC awake and starting with the system. Every order says what is true now, so applying one twice
// does no harm; the decisions (the setting, a game in front, one notice per question) were all made in Rust.

import { app, Menu, Notification, nativeImage, powerSaveBlocker, Tray } from 'electron';

type TrayOrder = {
  kind: 'tray';
  tooltip: string;
  needsYou: number;
  picture: string;
  picture16: string;
  menu: { show: string; quit: string };
};
type NotifyOrder = { kind: 'notify'; title: string; body: string; conversationId: string | null; toast: boolean; sound: boolean };
type Order = TrayOrder | NotifyOrder | { kind: 'awake'; on: boolean } | { kind: 'autostart'; on: boolean };

/** What the orders need from the window, which the shell owns. */
export type Window = {
  /** True while the person is looking at Faber: a notice then would only interrupt. */
  focused: () => boolean;
  /** Brings the window back (from the tray, or after a notice is clicked). */
  show: () => void;
  /** A test run shows nothing outside its window and changes nothing on the system. */
  hidden: boolean;
};

let tray: Tray | null = null;
let blocker: number | null = null;
// A notice the system still shows is kept here: one collected by the garbage collector loses its click.
const shown = new Set<Notification>();

function isOrder(value: unknown): value is Order {
  const kind = (value as { kind?: unknown } | null)?.kind;
  return kind === 'tray' || kind === 'notify' || kind === 'awake' || kind === 'autostart';
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

function startWithSystem(on: boolean, window: Window): void {
  // Only the installed app registers itself: a copy run from source would put a developer's Electron in the startup list.
  if (window.hidden || !app.isPackaged) return;
  app.setLoginItemSettings({ openAtLogin: on, args: on ? ['--autostart'] : [] });
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
  }
}

/** Lets the PC sleep and removes the icon, as the app quits. */
export function release(): void {
  keepAwake(false);
  tray?.destroy();
  tray = null;
}
