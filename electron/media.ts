import { protocol } from 'electron';
import type { Sidecar } from './sidecar.ts';

export const MEDIA_SCHEME = 'faber-media';
export const MEDIA_ORIGIN = `${MEDIA_SCHEME}://app`;
export const mediaScheme = { scheme: MEDIA_SCHEME, privileges: { standard: true, secure: true, stream: true } };

const path = /^faber-media:\/\/app\/([a-f0-9]{32})$/;
const headers = {
  'Cache-Control': 'no-store',
  'X-Content-Type-Options': 'nosniff',
  'Content-Security-Policy': "default-src 'none'; sandbox",
  'Referrer-Policy': 'no-referrer',
};

type Chunk = { body: string; type: string; range: string | null };
type Refusal = { error: string; length: number | null };

function refused(error: string, length: number | null = null): Response {
  return new Response(null, {
    status: length !== null ? 416 : error === 'error.media.closed' ? 403 : error === 'error.media.limit' ? 413 : 404,
    headers: {
      ...headers,
      'X-Faber-Media-Error': error,
      ...(length === null ? {} : { 'Content-Range': `bytes */${length}` }),
    },
  });
}

export function installMedia(host: () => Sidecar | null): void {
  protocol.handle(MEDIA_SCHEME, async (request) => {
    const token = path.exec(request.url)?.[1];
    const current = host();
    const dest = request.headers.get('sec-fetch-dest');
    if (!current || !token || request.method !== 'GET' || (dest !== null && !['image', 'video', 'audio'].includes(dest))) {
      return refused('error.media.closed');
    }
    try {
      const result = await current.call('media_chunk', { token, range: request.headers.get('range') }) as Chunk | Refusal;
      if ('error' in result) return refused(result.error, result.length);
      const body = Buffer.from(result.body, 'base64');
      return new Response(body, {
        status: result.range ? 206 : 200,
        headers: {
          ...headers,
          'Content-Type': result.type,
          'Content-Length': String(body.length),
          'Accept-Ranges': 'bytes',
          ...(result.range ? { 'Content-Range': result.range } : {}),
        },
      });
    } catch {
      return refused('error.media.gone');
    }
  });
}
