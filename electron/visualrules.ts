import { isIPv4 } from 'node:net';

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
    !(a === 192 && (b === 0 || b === 2 || b === 88 || b === 168)) &&
    !(a === 198 && (b === 18 || b === 19 || b === 51)) &&
    !(a === 203 && b === 0 && octets[2] === 113) &&
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
      (s0 & 0xe000) === 0x2000 &&
      !(s0 === 0x2001 && (s1 === 0x0db8 || s1 < 0x0200)) &&
      s0 !== 0x2002 &&
      !(s0 === 0x3fff && s1 < 0x1000) &&
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


export function publicAddress(address: string): boolean {
  return publicHttps(`https://${address.includes(':') ? `[${address}]` : address}/`);
}
