// Faber's browser pane in the Electron shell (R459): the views only a window can make. Every rule (which pages load, who may
// use the pane, when it closes, where it waits) lives in Rust (`host/src/pane/`); the host orders a view over the sidecar's
// pipes (`Order::Pane` in host/src/wire.rs) and this file opens, moves, loads and asks, and reports what the views tell. The
// contract is the doc of `Driver` in host/src/pane/driver.rs; the Tauri pane (`src-tauri/src/pane/`) is the behaviour to match.
//
// A view is a `WebContentsView` in the main window, on a private in-memory profile: no cookie, login, storage or cache reaches
// the disk. It has no preload, no Node, a sandbox and an isolated context; it opens no window, starts no download, asks for no
// permission and loads only web pages. Parked, it sits outside the window where Chromium keeps drawing it.

import { type BrowserWindow, type Rectangle, type Session, session, type WebContents, WebContentsView } from 'electron';
import { allowedRequest, allowedUrl, CALLS, plainProfile } from './panerules.ts';

type Name = 'main' | 'behind';

/** What the host orders (`Order::Pane`). */
export type PaneOrder = { kind: 'pane'; id?: number; view: Name } & (
  | { op: 'open'; profile: string }
  | { op: 'close' }
  | { op: 'place'; x: number; y: number; width: number; height: number }
  | { op: 'park' }
  | { op: 'load'; url: string }
  | { op: 'call'; method: string; params?: unknown }
  | { op: 'clear' }
);

/** What a view tells the host (`ViewNote`). */
type ViewNote = { kind: 'started'; url: string } | { kind: 'finished'; url: string } | { kind: 'moved'; url: string } | { kind: 'titled' } | { kind: 'gone' };

/** What the shell tells the host about the pane (`ShellNote::PaneAnswer` and `ShellNote::PaneEvent`). */
export type PaneNote =
  | { kind: 'paneAnswer'; id: number; ok?: unknown; err?: string }
  | { kind: 'paneEvent'; view: Name; event: ViewNote };

/** What the pane needs from the shell around it. */
export type PaneHost = {
  /** The window the views live in, when one is open. */
  window: () => BrowserWindow | null;
  /** Tells the host something. */
  note: (note: PaneNote) => void;
};

/** Where a view waits while nobody looks at it (`PARKED` and `PARKED_SIZE` in host/src/pane/driver.rs). */
const PARKED: Rectangle = { x: -20000, y: -20000, width: 1280, height: 800 };
/** Chromium's code for a navigation that was cancelled. */
const ABORTED = -3;
/** The channels of the screen the pane's events may use (`PaneEvent::wire` in host/src/pane/home.rs). */
export const SCREEN_CHANNELS: ReadonlySet<string> = new Set(['faber:site', 'faber:site-loading', 'faber:site-tabs', 'faber:site-busy']);

type Entry = {
  name: Name;
  profile: string;
  /** What keeps the profile in use while this view is open. */
  claim: object;
  view: WebContentsView;
  contents: WebContents;
  bounds: Rectangle;
  /** The view is open: it reports, and answers calls. Never true again once it closes. */
  open: boolean;
  /** A page started to load and has not ended, as far as the host was told. */
  loading: boolean;
  loadingUrl: string;
};

/** A profile in use: the session behind it and the views on it. */
type Held = { session: Session; users: Set<object>; ready: Promise<void> };

function isOrder(value: unknown): value is PaneOrder {
  const order = value as { kind?: unknown; view?: unknown; op?: unknown; id?: unknown } | null;
  return (
    order?.kind === 'pane' &&
    (order.view === 'main' || order.view === 'behind') &&
    typeof order.op === 'string' &&
    (order.id === undefined || typeof order.id === 'number')
  );
}

const reason = (failure: unknown): string => (failure instanceof Error ? failure.message : String(failure));

/** The part of a box the window can show: whole pixels, never empty. */
function whole(x: unknown, y: unknown, width: unknown, height: unknown): Rectangle {
  const numbers = [x, y, width, height];
  if (!numbers.every((n) => typeof n === 'number' && Number.isFinite(n))) throw new Error('the box is not valid');
  return { x: Math.round(x as number), y: Math.round(y as number), width: Math.max(1, Math.round(width as number)), height: Math.max(1, Math.round(height as number)) };
}

/** Everything a profile holds is cleared: its cookies, storage and caches, and the connections that may carry its sign-ins. */
async function wipe(profile: Session): Promise<void> {
  const results = await Promise.allSettled([profile.clearData(), profile.clearAuthCache(), profile.clearHostResolverCache(), profile.closeAllConnections()]);
  if (results.some((result) => result.status === 'rejected')) throw new Error('profile cleanup failed');
}

/** A session no page can ask anything of: no permission, no device, no download. Its certificate errors keep Chromium's answer. */
function guard(profile: Session): void {
  profile.setPermissionRequestHandler((_contents, _permission, answer) => answer(false));
  profile.setPermissionCheckHandler(() => false);
  profile.setDevicePermissionHandler(() => false);
  profile.setSpellCheckerEnabled(false);
  profile.webRequest.onBeforeRequest((details, callback) => callback({ cancel: !allowedRequest(details.url) }));
  profile.on('will-download', (event) => event.preventDefault());
}

export class Pane {
  private readonly host: PaneHost;
  private readonly views = new Map<Name, Entry>();
  /** The profiles the host named and the views on them. */
  private readonly held = new Map<string, Held>();
  /**
   * Electron cannot drop a session once it exists, and the host never names a profile twice. So a profile whose last view
   * closed is wiped and kept here, and the next one the host names is made on it: the session is private to one profile at a
   * time, and a pane made later starts clean (the wipe is awaited before its view opens).
   */
  private readonly spare: { session: Session; ready: Promise<void> }[] = [];
  private slots = 0;
  /** Counts the opens and closes of a view, so an open that was overtaken by a close never makes its view. */
  private readonly epoch: Record<Name, number> = { main: 0, behind: 0 };

  constructor(host: PaneHost) {
    this.host = host;
  }

  /** Whether `contents` is one of the pane's views: nothing the screen may ask of the shell is asked from there. */
  owns(contents: WebContents): boolean {
    for (const entry of this.views.values()) if (entry.contents === contents) return true;
    return false;
  }

  /** Keeps the views where the host put them as the window moves and changes size, and ends them with the window. */
  watch(window: BrowserWindow): void {
    const layout = () => this.layout();
    window.on('resize', layout);
    window.on('maximize', layout);
    window.on('unmaximize', layout);
    window.on('restore', layout);
    window.on('enter-full-screen', layout);
    window.on('leave-full-screen', layout);
    window.on('moved', layout);
    window.once('closed', () => this.windowClosed());
  }

  /** One order from the host. A call with an `id` is answered with a note; the others answer nothing. */
  order(value: unknown): void {
    if (!isOrder(value)) return;
    const name = value.view;
    if (value.id === undefined) {
      try {
        if (value.op === 'close') this.close(name);
        else if (value.op === 'place') this.move(name, whole(value.x, value.y, value.width, value.height));
        else if (value.op === 'park') this.move(name, PARKED);
      } catch {
        // A view that is not open takes no placing: the host hears of it when it asks for something.
      }
      return;
    }
    const id = value.id;
    let work: Promise<unknown>;
    try {
      work = this.run(value);
    } catch (failure) {
      work = Promise.reject(failure);
    }
    work.then(
      (ok) => this.host.note({ kind: 'paneAnswer', id, ok: ok ?? null }),
      (failure: unknown) => this.host.note({ kind: 'paneAnswer', id, err: reason(failure) }),
    );
  }

  /** Closes every view without a word to the host: the app is ending. */
  closeAll(): void {
    // A Map may lose the entry it is on while it is walked.
    this.close('main');
    this.close('behind');
  }

  private run(order: PaneOrder): Promise<unknown> {
    switch (order.op) {
      case 'open':
        return this.open(order.view, order.profile);
      case 'load':
        return Promise.resolve(this.load(order.view, order.url));
      case 'call':
        return this.call(order.view, order.method, order.params);
      case 'clear':
        return this.clear(order.view);
      default:
        return Promise.reject(new Error('that order has no answer'));
    }
  }

  private need(name: Name): Entry {
    const entry = this.views.get(name);
    if (!entry?.open) throw new Error('not open');
    return entry;
  }

  /** A session for a profile the host just named: the one already on it, or a wiped spare, or a new one. */
  private profile(name: string): Held {
    const known = this.held.get(name);
    if (known) return known;
    let spare = this.spare.pop();
    if (!spare) {
      // A name without `persist:` is a profile in memory only.
      const made = session.fromPartition(`faber-pane-slot-${++this.slots}`);
      guard(made);
      spare = { session: made, ready: Promise.resolve() };
    }
    const held: Held = { session: spare.session, users: new Set(), ready: spare.ready };
    this.held.set(name, held);
    return held;
  }

  private async open(name: Name, profile: string): Promise<void> {
    if (!plainProfile(profile)) throw new Error('the profile name is not valid');
    // A view opened again replaces the one before: an open the host gave up on may still have made it.
    this.close(name);
    const mine = this.epoch[name];
    const held = this.profile(profile);
    const claim = {};
    held.users.add(claim);
    try {
      await held.ready;
      const window = this.host.window();
      if (this.epoch[name] !== mine || !window || window.isDestroyed()) throw new Error("Faber's window is closed");
      const view = new WebContentsView({
        webPreferences: {
          session: held.session,
          sandbox: true,
          contextIsolation: true,
          nodeIntegration: false,
          nodeIntegrationInSubFrames: false,
          nodeIntegrationInWorker: false,
          webviewTag: false,
          webSecurity: true,
          allowRunningInsecureContent: false,
          spellcheck: false,
          backgroundThrottling: false,
          // A page that asks (alert, confirm, a file dialog) would hold a parked view forever.
          disableDialogs: true,
          navigateOnDragDrop: false,
        },
      });
      const contents = view.webContents;
      const entry: Entry = { name, profile, claim, view, contents, bounds: PARKED, open: true, loading: false, loadingUrl: '' };
      this.views.set(name, entry);
      this.guard(entry);
      view.setBounds(PARKED);
      window.contentView.addChildView(view);
      contents.debugger.attach('1.3');
      // Interception keeps Chromium's native file chooser from opening for pane pages.
      await contents.debugger.sendCommand('Page.enable');
      await contents.debugger.sendCommand('Page.setInterceptFileChooserDialog', { enabled: true, cancel: true });
      // Pages are told the browser they would have met anywhere, not the shell they run in.
      contents.setUserAgent(contents.getUserAgent().replace(/\s(?:Electron|Faber)\/\S+/gi, ''));
      await contents.loadURL('about:blank');
      if (this.views.get(name) !== entry || !entry.open) throw new Error('the browser closed');
    } catch (failure) {
      // The view, if it was made, closes with the profile's claim; an open that was overtaken leaves what is not its own.
      if (this.epoch[name] === mine) this.close(name);
      this.leave(profile, claim);
      throw failure;
    }
  }

  /** Everything a view refuses, reports or survives; all silent, since the host's rules decided what is worth a word. */
  private guard(entry: Entry): void {
    const { contents } = entry;
    const tell = (event: ViewNote) => {
      if (entry.open) this.host.note({ kind: 'paneEvent', view: entry.name, event });
    };
    // The page opens no window, whoever asks; a refused one is not reported.
    contents.setWindowOpenHandler(() => ({ action: 'deny' }));
    contents.on('will-attach-webview', (event) => event.preventDefault());
    // Only web pages load, whoever starts the navigation (the page, a redirect, the back button).
    const refuse = (event: { url: string; isMainFrame: boolean; preventDefault: () => void }) => {
      if (event.isMainFrame && !allowedUrl(event.url)) {
        event.preventDefault();
      }
    };
    contents.on('will-navigate', refuse);
    contents.on('will-redirect', refuse);
    contents.on('will-frame-navigate', refuse);
    contents.on('did-start-navigation', (details) => {
      if (!details.isMainFrame || details.isSameDocument) return;
      if (!allowedUrl(details.url)) {
        // A navigation the events above could not see (started from the browser's side): ended before it commits.
        if (!details.url.startsWith('about:') && !details.url.startsWith('chrome-error:')) contents.stop();
        return;
      }
      entry.loading = true;
      entry.loadingUrl = details.url;
      tell({ kind: 'started', url: details.url });
    });
    contents.on('did-redirect-navigation', (details) => {
      if (details.isMainFrame && entry.loading) entry.loadingUrl = details.url;
    });
    contents.on('did-finish-load', () => {
      const url = contents.getURL();
      if (!allowedUrl(url)) return;
      entry.loading = false;
      tell({ kind: 'finished', url });
    });
    const failed = (_event: Electron.Event, code: number, _description: string, address: string, isMainFrame: boolean) => {
      if (!isMainFrame || address !== entry.loadingUrl) return;
      // A load that failed ends like one that finished, as the system webview did. One cancelled by a newer page ends with
      // that page; one cancelled here ends the page it began.
      let url = address;
      if (code === ABORTED) {
        url = allowedUrl(contents.getURL()) ? contents.getURL() : address;
      }
      if (!entry.loading || !allowedUrl(url)) return;
      entry.loading = false;
      tell({ kind: 'finished', url });
    };
    contents.on('did-fail-load', failed);
    contents.on('did-fail-provisional-load', failed);
    contents.on('did-navigate-in-page', (_event, url, isMainFrame) => {
      if (isMainFrame && allowedUrl(url)) tell({ kind: 'moved', url });
    });
    contents.on('page-title-updated', () => tell({ kind: 'titled' }));
    contents.on('render-process-gone', () => this.gone(entry));
    contents.debugger.on('detach', () => this.gone(entry));
  }

  /** The view died or lost its page: the host hears it once, and the view is closed. */
  private gone(entry: Entry): void {
    if (!entry.open) return;
    this.host.note({ kind: 'paneEvent', view: entry.name, event: { kind: 'gone' } });
    if (this.views.get(entry.name) === entry) this.close(entry.name);
  }

  private move(name: Name, bounds: Rectangle): void {
    const entry = this.need(name);
    entry.bounds = bounds;
    entry.view.setBounds(bounds);
  }

  private layout(): void {
    for (const entry of this.views.values()) if (entry.open) entry.view.setBounds(entry.bounds);
  }

  private load(name: Name, url: string): void {
    const entry = this.need(name);
    if (!allowedUrl(url)) throw new Error("only web pages open in Faber's browser");
    // The page's end comes later as `finished`; a load that fails is reported there too.
    entry.contents.loadURL(url).catch(() => {});
  }

  private async call(name: Name, method: string, params: unknown): Promise<unknown> {
    const entry = this.need(name);
    if (!CALLS.has(method)) throw new Error('that DevTools call is not one Faber makes');
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      return await Promise.race([
        entry.contents.debugger.sendCommand(method, params ?? {}),
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => {
            if (method === 'Runtime.evaluate' && entry.contents.debugger.isAttached()) {
              void entry.contents.debugger.sendCommand('Runtime.terminateExecution').catch(() => {});
            }
            reject(new Error('the browser call timed out'));
          }, 10_000);
        }),
      ]);
    } finally {
      clearTimeout(timer);
    }
  }

  private async clear(name: Name): Promise<void> {
    const entry = this.need(name);
    const held = this.held.get(entry.profile);
    if (held) {
      held.ready = held.ready.then(() => wipe(held.session));
      await held.ready;
    }
  }

  /** Closes `name` and everything its page holds; nothing more is reported for it. Does nothing for a view that is not open. */
  private close(name: Name): void {
    this.epoch[name]++;
    const entry = this.views.get(name);
    if (!entry) return;
    this.views.delete(name);
    entry.open = false;
    const { contents, view } = entry;
    try {
      if (contents.debugger.isAttached()) contents.debugger.detach();
    } catch {}
    try {
      this.host.window()?.contentView.removeChildView(view);
    } catch {}
    if (!contents.isDestroyed()) contents.close();
    this.leave(entry.profile, entry.claim);
  }

  /** A view no longer uses `profile`; once none does, the profile is wiped and kept for the next one. */
  private leave(profile: string, claim: object): void {
    const held = this.held.get(profile);
    if (!held || !held.users.delete(claim)) return;
    if (held.users.size > 0) return;
    this.held.delete(profile);
    const ready = held.ready.then(() => wipe(held.session));
    // A failed cleanup retires this session instead of sharing its data with another profile.
    void ready.then(() => this.spare.push({ session: held.session, ready: Promise.resolve() }), () => {});
  }

  /** The window closed: its views are gone with it, and the host hears so before they are dropped. */
  private windowClosed(): void {
    for (const entry of this.views.values()) this.gone(entry);
    this.closeAll();
  }
}
