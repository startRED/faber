// The Electron shell's `faber-visual://app/<id>.html` page server and frame network guard (R459).
// The host owns the single-use page store and its CSP (`host/src/visual.rs`); this file only serves the bytes
// the host returns and cancels any frame navigation or subresource request the policy forbids.

import { type BrowserWindow, protocol, session, type WebContents, type WebFrameMain } from 'electron';
import { isIPv4 } from 'node:net';
import type { Sidecar } from './sidecar.ts';

export const VISUAL_SCHEME = 'faber-visual';
export const VISUAL_ORIGIN = `${VISUAL_SCHEME}://app`;

export const visualScheme = {
  scheme: VISUAL_SCHEME,
  privileges: { standard: true, secure: true, supportFetchAPI: true },
};

const PAGE_URL = /^faber-visual:\/\/app\/[0-9a-f]{32}\.html$/;
const BLOCKED_SUFFIXES = ['localhost', 'local', 'internal', 'lan', 'home', 'tauri.localhost', 'ipc.localhost'];

export function navigationAllowed(url: string): boolean {
  return url === 'about:blank' || PAGE_URL.test(url);
}

function publicIpv4(octets: number[]): boolean {
  const [a = 0, b = 0] = octets;
  return (
    octets.length === 4 &&
    a !== 0 &&
    a !== 10 &&
    a !== 127 &&
    !(a === 100 && b >= 64 && b <= 127) &&
    !(a === 169 && b === 254) &&
    !(a === 172 && b >= 16 && b <= 31) &&
    !(a === 192 && b === 168) &&
    a < 224
  );
}

function parseIpv6(host: string): number[] | null {
  const body = host.slice(1, -1).toLowerCase();
  if (!body || body.includes(':::')) return null;
  const parts = body.split('::');
  if (parts.length > 2) return null;
  const expand = (chunk: string): number[] | null => {
    if (!chunk) return [];
    const out: number[] = [];
    for (const piece of chunk.split(':')) {
      if (piece.includes('.')) {
        if (!isIPv4(piece)) return null;
        const nums = piece.split('.').map(Number);
        out.push(((nums[0] ?? 0) << 8) | (nums[1] ?? 0), ((nums[2] ?? 0) << 8) | (nums[3] ?? 0));
      } else {
        if (!/^[0-9a-f]{1,4}$/.test(piece)) return null;
        out.push(Number.parseInt(piece, 16));
      }
    }
    return out;
  };
  const left = expand(parts[0] ?? '');
  const right = parts.length === 2 ? expand(parts[1] ?? '') : [];
  if (!left || !right) return null;
  if (parts.length === 1) return left.length === 8 ? left : null;
  const fill = 8 - left.length - right.length;
  if (fill < 1) return null;
  return [...left, ...Array.from({ length: fill }, () => 0), ...right];
}

/** Matches `faber_engine::visual::public_https`: only public HTTPS hosts, never local or private addresses. */
export function publicHttps(source: string): boolean {
  let url: URL;
  try {
    url = new URL(source);
  } catch {
    return false;
  }
  if (url.protocol !== 'https:' || url.username !== '' || url.password !== '') return false;
  const host = url.hostname.toLowerCase().replace(/\.+$/, '');
  if (!host) return false;
  if (host.startsWith('[') && host.endsWith(']')) {
    const seg = parseIpv6(host);
    if (!seg || seg.length !== 8) return false;
    const s0 = seg[0] ?? 0;
    const s1 = seg[1] ?? 0;
    const s5 = seg[5] ?? 0;
    const s6 = seg[6] ?? 0;
    const s7 = seg[7] ?? 0;
    const firstFiveZero = seg.slice(0, 5).every((n) => n === 0);
    if (firstFiveZero && s5 === 0xffff) {
      return publicIpv4([s6 >> 8, s6 & 0xff, s7 >> 8, s7 & 0xff]);
    }
    const firstSixZero = firstFiveZero && s5 === 0;
    const nat64 = s0 === 0x64 && s1 === 0xff9b && seg.slice(2, 6).every((n) => n === 0);
    return (
      !firstSixZero &&
      !nat64 &&
      (s0 & 0xfe00) !== 0xfc00 &&
      (s0 & 0xffc0) !== 0xfe80 &&
      (s0 & 0xffc0) !== 0xfec0 &&
      (s0 & 0xff00) !== 0xff00
    );
  }
  if (isIPv4(host)) {
    return publicIpv4(host.split('.').map(Number));
  }
  return (
    host.includes('.') &&
    !BLOCKED_SUFFIXES.some((suffix) => host === suffix || host.endsWith(`.${suffix}`))
  );
}

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

  session.defaultSession.webRequest.onBeforeRequest((details, callback) => {
    if (details.resourceType === 'mainFrame') {
      callback({ cancel: details.url.startsWith(`${VISUAL_SCHEME}:`) });
      return;
    }
    const main = screen()?.webContents.mainFrame;
    const frame = details.frame;
    if (frame && main && !sameFrame(frame, main)) {
      const directChild = sameFrame(frame.parent, main);
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
      const allowed = livePages.has(frame.url) && publicHttps(details.url);
      callback({ cancel: !allowed });
      return;
    }
    if (details.url.startsWith(`${VISUAL_SCHEME}:`)) {
      callback({ cancel: !(details.resourceType === 'subFrame' && details.method === 'GET' && navigationAllowed(details.url)) });
      return;
    }
    callback({ cancel: false });
  });
}
