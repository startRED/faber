// The Electron shell's `faber-visual://app/<id>.html` page server and frame network guard (R459).
// The host owns the single-use page store and its CSP (`host/src/visual.rs`); this file only serves the bytes
// the host returns and cancels any frame navigation or subresource request the policy forbids.

import { type BrowserWindow, protocol, session, type WebContents, type WebFrameMain } from 'electron';
import { lookup } from 'node:dns/promises';
import { navigationAllowed, publicAddress, publicHttps } from './visualrules.ts';
import type { Sidecar } from './sidecar.ts';

export const VISUAL_SCHEME = 'faber-visual';
export const VISUAL_ORIGIN = `${VISUAL_SCHEME}://app`;

export const visualScheme = {
  scheme: VISUAL_SCHEME,
  privileges: { standard: true, secure: true, supportFetchAPI: true },
};

function sameFrame(a: WebFrameMain | null | undefined, b: WebFrameMain | null | undefined): boolean {
  return Boolean(a && b && a.processId === b.processId && a.routingId === b.routingId);
}

/** Confines child-frame navigations on the screen's `WebContents` to direct `faber-visual://app/<id>.html` pages. */
export function guardVisualFrames(contents: WebContents): void {
  contents.on('will-frame-navigate', (details) => {
    if (details.isMainFrame) {
      if (details.url.startsWith(`${VISUAL_SCHEME}:`)) details.preventDefault();
      return;
    }
    if (!sameFrame(details.frame?.parent, contents.mainFrame) || !navigationAllowed(details.url)) {
      details.preventDefault();
    }
  });
}

const DENIED_HEADERS = {
  'Content-Type': 'text/html; charset=utf-8',
  'Content-Security-Policy': "default-src 'none'; sandbox",
  'Cache-Control': 'no-store',
  'Referrer-Policy': 'no-referrer',
  'X-Content-Type-Options': 'nosniff',
};

/** Serves single-use `faber-visual://app/<id>.html` pages from the host and filters child-frame requests. */
export function installVisual(host: () => Sidecar | null, screen: () => BrowserWindow | null): void {
  const livePages = new Set<string>();

  protocol.handle(VISUAL_SCHEME, async (request) => {
    const dest = request.headers.get('sec-fetch-dest');
    const valid =
      request.method === 'GET' &&
      (dest === 'iframe' || dest === null) &&
      request.url !== 'about:blank' &&
      navigationAllowed(request.url);
    const current = host();
    if (!valid || !current) return new Response(new Uint8Array(), { status: 403, headers: DENIED_HEADERS });
    try {
      const doc = (await current.call('visual_document', { path: request.url })) as { body: string; csp: string };
      if (doc.csp.includes('connect-src https:')) {
        if (livePages.size >= 64) {
          const oldest = livePages.values().next().value;
          if (oldest) livePages.delete(oldest);
        }
        livePages.add(request.url);
      } else {
        livePages.delete(request.url);
      }
      return new Response(Buffer.from(doc.body, 'base64'), {
        status: 200,
        headers: {
          'Content-Type': 'text/html; charset=utf-8',
          'Content-Security-Policy': doc.csp,
          'Cache-Control': 'no-store',
          'Referrer-Policy': 'no-referrer',
          'X-Content-Type-Options': 'nosniff',
        },
      });
    } catch {
      return new Response(new Uint8Array(), { status: 403, headers: DENIED_HEADERS });
    }
  });

  const leases = new Set<number>();
  session.defaultSession.webRequest.onCompleted((details) => leases.delete(details.id));
  session.defaultSession.webRequest.onErrorOccurred((details) => leases.delete(details.id));
  session.defaultSession.webRequest.onBeforeRequest((details, callback) => {
    if (details.resourceType === 'mainFrame') {
      const own = details.url.startsWith('faber://');
      const blank = details.url === 'about:blank' && sameFrame(details.frame, screen()?.webContents.mainFrame);
      callback({ cancel: !own && !blank });
      return;
    }
    const main = screen()?.webContents.mainFrame;
    const frame = details.frame;
    if (leases.has(details.id) || (frame && main && !sameFrame(frame, main))) {
      const directChild = leases.has(details.id) || Boolean(frame && sameFrame(frame.parent, main));
      if (details.resourceType === 'subFrame') {
        callback({ cancel: !(directChild && details.method === 'GET' && navigationAllowed(details.url)) });
        return;
      }
      if (!directChild || (details.method !== 'GET' && details.method !== 'HEAD')) {
        callback({ cancel: true });
        return;
      }
      if (details.url.startsWith('data:') || details.url.startsWith('blob:')) {
        callback({ cancel: false });
        return;
      }
      const allowed = (leases.has(details.id) || (frame && livePages.has(frame.url))) && publicHttps(details.url);
      if (!allowed) { callback({ cancel: true }); return; }
      leases.add(details.id);
      // DNS can change between this lookup and Chromium's connection (DNS rebinding).
      const hostname = new URL(details.url).hostname.replace(/^\[|\]$/g, '');
      void lookup(hostname, { all: true }).then(
        (addresses) => callback({ cancel: addresses.length === 0 || addresses.some(({ address }) => !publicAddress(address)) }),
        () => callback({ cancel: true }),
      );
      return;
    }
    if (details.url.startsWith(`${VISUAL_SCHEME}:`)) {
      callback({ cancel: !(frame && details.resourceType === 'subFrame' && details.method === 'GET' && navigationAllowed(details.url)) });
      return;
    }
    callback({ cancel: !frame });
  });
}
