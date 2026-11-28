// Faber's browser pane in the Electron shell (R459): the views only a window can make. Every rule (which pages load, who may
// use the pane, when it closes, where it waits) lives in Rust (`host/src/pane/`); the host orders a view over the sidecar's
// pipes (`Order::Pane` in host/src/wire.rs) and this file opens, moves, loads and asks, and reports what the views tell. The
// contract is the doc of `Driver` in host/src/pane/driver.rs; the Tauri pane (`src-tauri/src/pane/`) is the behaviour to match.
//
// A view is a `WebContentsView` in the main window. Ordinary profiles are in memory; extension development uses a separate
// disk-backed session. Views have no preload or Node, use a sandbox and an isolated context, and open no popups or downloads.

import { type BrowserWindow, type Rectangle, type Session, session, type WebContents, WebContentsView } from 'electron';
import { allowedRequest, allowedUrl, CALLS, plainProfile } from './panerules.ts';
import { PaneExtensions } from './pane-extensions.ts';

type Name = 'main' | 'behind' | 'capture';

/** What the host orders (`Order::Pane`). */
export type PaneOrder = { kind: 'pane'; id?: number; view: Name } & (
  | { op: 'open'; profile: string; development?: string }
  | { op: 'close' }
  | { op: 'place'; x: number; y: number; width: number; height: number }
  | { op: 'park' }
  | { op: 'load'; url: string }
  | { op: 'loadCapture'; url: string; blockedHosts: string[] }
  | { op: 'call'; method: string; params?: unknown }
  | { op: 'clear' }
);

/** What a view tells the host (`ViewNote`). */
type ViewNote = { kind: 'started'; url: string; navigation: number } | { kind: 'finished'; url: string; navigation: number } | { kind: 'moved'; url: string } | { kind: 'titled' } | { kind: 'gone' } | { kind: 'saw'; line: string };

/** What the shell tells the host about the pane (`ShellNote::PaneAnswer` and `ShellNote::PaneEvent`). */
export type PaneNote =
  | { kind: 'paneAnswer'; id: number; ok?: unknown; err?: string }
  | { kind: 'paneEvent'; view: Name; event: ViewNote };

/** What the pane needs from the shell around it. */
export type PaneHost = {
  /** The window the views live in, when one is open. */
  window: () => BrowserWindow | null;
  appControl?: (conversation: string, input: unknown) => Promise<unknown>;
  extensionStorage?: () => string | undefined;
  extensionRegistry?: string;
  extensionBlockedFolders?: readonly string[];
  /** Tells the host something. */
  note: (note: PaneNote) => void;
};

/** Where a view waits while nobody looks at it (`PARKED` and `PARKED_SIZE` in host/src/pane/driver.rs). */
const PARKED: Rectangle = { x: -20000, y: -20000, width: 1280, height: 800 };
/** The channels of the screen the pane's events may use (`PaneEvent::wire` in host/src/pane/home.rs). */
export const SCREEN_CHANNELS: ReadonlySet<string> = new Set(['faber:site', 'faber:site-loading', 'faber:site-tabs', 'faber:site-busy']);

type Entry = {
  name: Name;
  profile: string;
  development?: string;
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
  navigation: number;
  capture?: CapturePolicy;
};

type CapturePolicy = { origin?: string; blockedHosts: readonly string[] };

/** A profile in use: the session behind it and the views on it. */
type Held = { session: Promise<Session>; users: Set<object>; development?: string };

function isOrder(value: unknown): value is PaneOrder {
  const order = value as { kind?: unknown; view?: unknown; op?: unknown; id?: unknown } | null;
  return (
    order?.kind === 'pane' &&
    (order.view === 'main' || order.view === 'behind' || order.view === 'capture') &&
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
function guard(profile: Session, saw: (contents: WebContents, line: string) => void, permits?: (url: string) => boolean): void {
  profile.setPermissionRequestHandler((_contents, _permission, answer) => answer(false));
  profile.setPermissionCheckHandler(() => false);
  profile.setDevicePermissionHandler(() => false);
  profile.setSpellCheckerEnabled(false);
  guardRequests(profile, permits);
  profile.on('will-download', (event, item, contents) => {
    event.preventDefault();
    saw(contents, `download blocked: ${item.getFilename().slice(0, 80)}`);
  });
}

function guardRequests(profile: Session, permits?: (url: string) => boolean): void {
  profile.webRequest.onBeforeRequest(({ url }, callback) => {
    const allowed = permits
      ? permits(url) && (url.startsWith('chrome-extension:') || allowedRequest(url))
      : allowedRequest(url);
    callback({ cancel: !allowed });
  });
}

function captureUrl(address: string, blockedHosts: readonly string[]): URL | undefined {
  if (!/^https?:\/\/[^/\\]/i.test(address) || /[\u0000-\u001f\u007f-\u009f]/.test(address) || !allowedUrl(address)) return;
  const url = new URL(address);
  const host = url.hostname;
  if (url.username || url.password || !['localhost', '127.0.0.1', '[::1]'].includes(host)) return;
  if (!blockedHosts.some((entry) => host === entry || host.endsWith(`.${entry}`))) return url;
}

/** An address as the host may hear it: host and path, without its query, fragment or sign-in part, and without path parts that look like an id or a token. */
function placeOf(url: string): string {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return '?';
    const path = parsed.pathname.split('/').map((part) => (/^[A-Za-z0-9_-]{16,}$|\d{5,}|;/.test(part) ? '…' : part)).join('/');
    return `${parsed.host}${path}`.slice(0, 120);
  } catch {
    return '?';
  }
}

/** What a page wrote, with each address cut down to {@link placeOf} (a query often carries a token); the host masks, then cuts. */
const plain = (text: string): string => text.replace(/\b(?:https?|data|blob|javascript):[^\s"'<>)]*/gi, (u) => placeOf(u)).slice(0, 400);

export class Pane {
  private readonly host: PaneHost;
  private readonly extensions: PaneExtensions;
  private navigation = 0;
  private readonly views = new Map<Name, Entry>();
  private readonly opening = new Map<Name, Promise<void>>();
  /** The profiles the host named and the views on them. */
  private readonly held = new Map<string, Held>();
  /**
   * Electron cannot drop a session once it exists, and the host never names a profile twice. So a profile whose last view
   * closed is wiped and kept here, and the next one the host names is made on it: the session is private to one profile at a
   * time, and a pane made later starts clean (the wipe is awaited before its view opens).
   */
  private readonly spare: Session[] = [];
  private slots = 0;
  /** Counts the opens and closes of a view, so an open that was overtaken by a close never makes its view. */
  private readonly epoch: Record<Name, number> = { main: 0, behind: 0, capture: 0 };

  constructor(host: PaneHost) {
    this.host = host;
    this.extensions = new PaneExtensions(host.extensionStorage ?? (() => undefined), (folder, permits) => {
      const made = session.fromPath(folder, { cache: false });
      guard(made, this.saw, permits);
      return made;
    }, wipe, host.extensionRegistry, host.extensionBlockedFolders);
  }

  private readonly saw = (contents: WebContents, line: string): void => {
    for (const entry of this.views.values()) if (entry.contents === contents) this.tell(entry, { kind: 'saw', line });
  };

  stopExtensions(): Promise<void> {
    const cleared = this.extensions.mode(false);
    for (const entry of this.views.values()) if (entry.development) this.gone(entry);
    for (const [profile, held] of this.held) if (held.development) this.held.delete(profile);
    return cleared;
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
    this.close('capture');
  }

  private run(order: PaneOrder): Promise<unknown> {
    switch (order.op) {
      case 'open': {
        const pending = this.open(order.view, order.profile, order.development);
        this.opening.set(order.view, pending);
        return pending.finally(() => {
          if (this.opening.get(order.view) === pending) this.opening.delete(order.view);
        });
      }
      case 'load':
        return this.load(order.view, order.url);
      case 'loadCapture':
        if (order.view !== 'capture') throw new Error('not a capture view');
        return this.load(order.view, order.url, order.blockedHosts);
      case 'call':
        if (order.method === 'Faber.extensionMode') {
          const params = order.params as { enabled?: unknown } | null;
          if (typeof params?.enabled !== 'boolean') throw new Error('invalid extension mode');
          return params.enabled ? this.extensions.mode(true) : this.stopExtensions();
        }
        if (order.method === 'Faber.extensions') return this.extensions.act(order.params);
        if (order.method === 'Faber.forgetExtensions') {
          const scope = (order.params as { scope?: unknown } | null)?.scope;
          for (const entry of this.views.values()) if (entry.development && entry.development === scope) this.gone(entry);
          return this.extensions.forget(scope);
        }
        if (order.method === 'Faber.appControl') {
          const p = order.params as {conversation: string; input: unknown};
          if (!this.host.appControl) throw new Error('app control unavailable');
          return this.host.appControl(p.conversation, p.input);
        }
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
  private profile(name: string, development?: string, capture?: CapturePolicy): Held {
    const known = this.held.get(name);
    if (known) return known;
    let pending: Promise<Session>;
    if (development) {
      pending = this.extensions.acquire(development);
    } else {
      // A name without `persist:` is a profile in memory only.
      const permits = capture && ((url: string) => capture.origin !== undefined && captureUrl(url, capture.blockedHosts) !== undefined);
      let made = this.spare.pop();
      if (!made) {
        made = session.fromPartition(`faber-pane-slot-${++this.slots}`);
        guard(made, this.saw, permits);
      } else {
        guardRequests(made, permits);
      }
      pending = Promise.resolve(made);
    }
    const held: Held = { session: pending, users: new Set(), development };
    this.held.set(name, held);
    return held;
  }

  private async open(name: Name, profile: string, development?: string): Promise<void> {
    if (!plainProfile(profile)) throw new Error('the profile name is not valid');
    const capture: CapturePolicy | undefined = name === 'capture' ? { blockedHosts: [] } : undefined;
    if (capture && (development !== undefined || this.held.has(profile))) throw new Error('capture requires its own profile');
    if (development !== undefined) profile = 'development:' + this.extensions.scope(development);
    // A view opened again replaces the one before: an open the host gave up on may still have made it.
    this.close(name);
    const mine = this.epoch[name];
    const held = this.profile(profile, development, capture);
    const claim = {};
    held.users.add(claim);
    try {
      const session = await held.session;
      const window = this.host.window();
      if (this.epoch[name] !== mine || this.held.get(profile) !== held || !window || window.isDestroyed()) throw new Error('the browser closed');
      const view = new WebContentsView({
        webPreferences: {
          session,
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
      const entry: Entry = { name, profile, development, claim, view, contents, bounds: PARKED, open: true, loading: false, loadingUrl: '', navigation: 0, capture };
      this.views.set(name, entry);
      this.guard(entry);
      view.setBounds(PARKED);
      window.contentView.addChildView(view);
      await this.initialize(entry);
      if (this.views.get(name) !== entry || !entry.open) throw new Error('the browser closed');
    } catch (failure) {
      // The view, if it was made, closes with the profile's claim; an open that was overtaken leaves what is not its own.
      if (this.epoch[name] === mine) this.close(name);
      this.leave(profile, claim);
      throw failure;
    }
  }

  private async initialize(entry: Entry): Promise<void> {
    const { contents } = entry;
    // The first navigation starts the renderer before the DevTools domains wait for it.
    await contents.loadURL('about:blank');
    if (!entry.open) throw new Error('the browser closed');
    contents.debugger.attach('1.3');
    // Interception keeps Chromium's native file chooser from opening for pane pages.
    await contents.debugger.sendCommand('Page.enable');
    this.listen(entry);
    await contents.debugger.sendCommand('Network.enable', { maxTotalBufferSize: 1, maxResourceBufferSize: 1 });
    await contents.debugger.sendCommand('Runtime.enable');
    await contents.debugger.sendCommand('Page.setInterceptFileChooserDialog', { enabled: true, cancel: true });
    // Pages are told the browser they would have met anywhere, not the shell they run in.
    contents.setUserAgent(contents.getUserAgent().replace(/\s(?:Electron|Faber)\/\S+/gi, ''));
  }

  /** Everything a view refuses, reports or survives; all silent, since the host's rules decided what is worth a word. */
  private tell(entry: Entry, event: ViewNote): void {
    if (entry.open && entry.name !== 'capture') this.host.note({ kind: 'paneEvent', view: entry.name, event });
  }

  private guard(entry: Entry): void {
    const { contents } = entry;
    const tell = (event: ViewNote) => this.tell(entry, event);
    // The page opens no window, whoever asks; the host is told where it tried to go (address without its query).
    contents.setWindowOpenHandler((details) => {
      tell({ kind: 'saw', line: `popup blocked: ${placeOf(details.url)}` });
      return { action: 'deny' };
    });
    contents.on('will-attach-webview', (event) => event.preventDefault());
    // Only web pages load, whoever starts the navigation (the page, a redirect, the back button).
    const { capture } = entry;
    const permits = (url: string, main = true) => {
      if (!capture) return allowedUrl(url);
      const local = captureUrl(url, capture.blockedHosts);
      return capture.origin !== undefined && local !== undefined && (!main || local.origin === capture.origin);
    };
    const refuse = (event: { url: string; isMainFrame: boolean; preventDefault: () => void }) => {
      if ((capture || event.isMainFrame) && !permits(event.url, event.isMainFrame)) {
        event.preventDefault();
      }
    };
    contents.on('will-navigate', refuse);
    contents.on('will-redirect', refuse);
    contents.on('will-frame-navigate', refuse);
    contents.on('did-start-navigation', (details) => {
      if (!details.isMainFrame || details.isSameDocument) return;
      if (!permits(details.url)) {
        // A navigation the events above could not see (started from the browser's side): ended before it commits.
        if (!details.url.startsWith('about:') && !details.url.startsWith('chrome-error:')) contents.stop();
        return;
      }
      entry.loading = true;
      entry.loadingUrl = details.url;
      entry.navigation = ++this.navigation;
      tell({ kind: 'started', url: details.url, navigation: entry.navigation });
    });
    contents.on('did-redirect-navigation', (details) => {
      if (details.isMainFrame && entry.loading) entry.loadingUrl = details.url;
    });
    contents.on('did-stop-loading', () => {
      if (!entry.loading || contents.isLoading()) return;
      const current = contents.getURL();
      const url = allowedUrl(current) ? current : entry.loadingUrl;
      if (!allowedUrl(url)) return;
      entry.loading = false;
      tell({ kind: 'finished', url, navigation: entry.navigation });
    });
    contents.on('did-navigate-in-page', (_event, url, isMainFrame) => {
      if (isMainFrame && allowedUrl(url)) tell({ kind: 'moved', url });
    });
    contents.on('page-title-updated', () => tell({ kind: 'titled' }));
    contents.on('render-process-gone', () => this.gone(entry));
    contents.debugger.on('detach', () => this.gone(entry));
  }

  /**
   * What happened on the page that the agent did not ask about, as short lines for the host (`ViewNote::Saw`): console errors,
   * failed requests and error answers (method, host, path and status only: never headers, cookies, bodies or queries).
   */
  private listen(entry: Entry): void {
    const asked = new Map<string, string>();
    const tell = (line: string) => this.tell(entry, { kind: 'saw', line });
    entry.contents.debugger.on('message', (_event, method, params: Record<string, any>) => {
      if (method === 'Network.requestWillBeSent') {
        if (asked.size >= 100) asked.delete(asked.keys().next().value as string);
        asked.set(params.requestId, `${/^[A-Z]{1,8}$/.test(params.request.method) ? params.request.method : '?'} ${placeOf(params.request.url)}`);
      } else if (method === 'Network.responseReceived') {
        if (params.response.status >= 400) tell(`request failed: ${asked.get(params.requestId) ?? placeOf(params.response.url)} ${params.response.status}`);
      } else if (method === 'Network.loadingFailed') {
        if (!params.canceled && params.errorText !== 'net::ERR_BLOCKED_BY_CLIENT') tell(`request failed: ${asked.get(params.requestId) ?? '?'} ${String(params.errorText).replace(/^net::/, '')}`);
      } else if (method === 'Runtime.consoleAPICalled') {
        if (params.type === 'error') tell(`console error: ${plain((params.args as { value?: unknown; description?: string }[]).map((a) => String(a.value ?? a.description ?? '')).join(' '))}`);
      } else if (method === 'Runtime.exceptionThrown') {
        tell(`page error: ${plain(String(params.exceptionDetails?.exception?.description ?? params.exceptionDetails?.text ?? '').split('\n')[0] ?? '')}`);
      }
    });
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

  private async load(name: Name, url: string, blockedHosts?: string[]): Promise<void> {
    if (!allowedUrl(url)) throw new Error("only web pages open in Faber's browser");
    const epoch = this.epoch[name];
    await this.opening.get(name);
    if (this.epoch[name] !== epoch) throw new Error('the browser closed');
    const entry = this.need(name);
    if (entry.capture) {
      if (!Array.isArray(blockedHosts)) throw new Error('missing capture block list');
      const parsed = captureUrl(url, blockedHosts);
      if (!parsed) throw new Error('capture requires an unblocked local app');
      entry.capture.blockedHosts = blockedHosts;
      entry.capture.origin = parsed.origin;
      await entry.contents.loadURL(url);
      return;
    }
    // The page's end comes later as `finished`; a load that fails is reported there too.
    void entry.contents.loadURL(url).catch(() => {});
  }

  private async call(name: Name, method: string, params: unknown): Promise<unknown> {
    const entry = this.need(name);
    if (!CALLS.has(method)) throw new Error('that DevTools call is not one Faber makes');
    if (entry.development && (method === 'Network.setCookie' || method === 'Network.setCookies')) {
      throw new Error('sign-ins cannot be imported into an extension profile');
    }
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
    if (entry.development) return this.extensions.clear(entry.development);
    const held = this.held.get(entry.profile);
    if (held) {
      const clean = held.session.then(async (session) => {
        await wipe(session);
        return session;
      });
      held.session = clean;
      await clean;
    }
  }

  /** Closes `name` and everything its page holds; nothing more is reported for it. Does nothing for a view that is not open. */
  private close(name: Name): void {
    this.epoch[name]++;
    const entry = this.views.get(name);
    if (!entry) return;
    this.views.delete(name);
    entry.open = false;
    if (entry.capture) delete entry.capture.origin;
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
    if (held.development) {
      void this.extensions.release(held.development).catch(() => {});
      return;
    }
    const ready = held.session.then(async (session) => {
      await wipe(session);
      return session;
    });
    // A failed cleanup retires this session instead of sharing its data with another profile.
    void ready.then((session) => this.spare.push(session), () => {});
  }

  /** The window closed: its views are gone with it, and the host hears so before they are dropped. */
  private windowClosed(): void {
    for (const entry of this.views.values()) this.gone(entry);
    this.closeAll();
  }
}
