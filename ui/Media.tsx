import { createContext, createEffect, createSignal, For, onCleanup, Show, useContext, type Accessor } from 'solid-js';
import { render } from 'solid-js/web';
import { Icon } from './Glyphs.tsx';
import { failureText, t } from './i18n.ts';
import { api, failureOf } from './api.ts';
import type { ReplyWebOptions } from './ReplyWebMedia.tsx';
import { openViewer } from './Viewer.tsx';

const pictures = /\.(?:png|jpe?g|gif|webp|avif|bmp|svg)$/i;
const videos = /\.(?:mp4|webm)$/i;
const sounds = /\.(?:mp3|wav|ogg|m4a|flac|opus)$/i;

export function mediaKind(path: string) {
  return pictures.test(path) ? 'picture' : videos.test(path) ? 'video' : sounds.test(path) ? 'audio' : undefined;
}

export interface MediaRef {
  path: string;
  name: string;
  kind: NonNullable<ReturnType<typeof mediaKind>>;
}

export function localMedia(path: string, name = '', folder = ''): MediaRef | undefined {
  const windows = /^[a-z]:[\\/]/i.test(folder);
  const slashes = (value: string) => windows ? value.replaceAll('\\', '/') : value;
  path = path.trim();
  if (path.startsWith('file://')) {
    try {
      const url = new URL(path);
      if (url.hostname || url.search || url.hash) return;
      path = decodeURIComponent(url.pathname);
      if (windows && /^\/[a-z]:\//i.test(path)) path = path.slice(1);
    } catch { return; }
  }
  path = slashes(path);
  const prefix = slashes(folder).replace(/\/$/, '');
  const rootSpelling = (value: string) => windows ? value.charAt(0).toLowerCase() + value.slice(1) : value;
  if (prefix && rootSpelling(path).startsWith(rootSpelling(prefix) + '/')) path = path.slice(prefix.length + 1);
  path = path.replace(/^(?:\.\/)+/, '');
  if (!path || path.length > 300 || /[:<>?"|*#]/.test(path) || path.startsWith('/') || path.split('/').includes('..')) return;
  const kind = mediaKind(path);
  if (kind) return { path, name: name.trim() || path.split('/').at(-1)!, kind };
}

let playing: HTMLMediaElement | undefined;

export function pauseReplyMedia() {
  playing?.pause();
  playing = undefined;
}

function refusal(error: unknown) {
  const key = error && typeof error === 'object' && 'key' in error ? error.key : error;
  return key === 'error.media.gone' || key === 'error.media.path';
}

export function mediaSession(id: string, folder: string, web?: Accessor<ReplyWebOptions | undefined>) {
  let lease: Promise<string> | undefined;
  let open = true;
  let requested = 0;
  let closePopup: (() => void) | undefined;
  let observer: IntersectionObserver | undefined;
  const elements = new Map<Element, (visible: boolean) => void>();
  const refusals = new Map<string, unknown>();
  const visibility = () => {
    observer?.disconnect();
    for (const [element, visible] of elements) {
      visible(false);
      if (!document.hidden) observer?.observe(element);
    }
  };
  const stopWatching = () => {
    observer?.disconnect();
    observer = undefined;
    document.removeEventListener('visibilitychange', visibility);
  };
  const release = (url: string) => void lease?.then((value) => api.mediaRelease(value, url)).catch(() => {});
  const enter = () => lease ??= api.mediaEnter(id).catch((error: unknown) => {
    lease = undefined;
    throw error;
  });
  const session = {
    folder,
    web,
    retry(path: string) {
      if (!refusal(refusals.get(path))) refusals.delete(path);
    },
    observe(element: Element, visible: (visible: boolean) => void) {
      if (!open) return () => {};
      if (!observer) {
        observer = new IntersectionObserver((entries) => {
          for (const entry of entries) elements.get(entry.target)?.(entry.isIntersecting && !document.hidden);
        });
        document.addEventListener('visibilitychange', visibility);
      }
      elements.set(element, visible);
      if (!document.hidden) observer.observe(element);
      return () => {
        observer?.unobserve(element);
        elements.delete(element);
        if (!elements.size) stopWatching();
      };
    },
    async grant(path: string) {
      if (!open) throw 'error.media.closed';
      if (refusals.has(path)) throw refusals.get(path);
      await enter();
      if (!open) throw 'error.media.closed';
      if (refusals.has(path)) throw refusals.get(path);
      const url = await api.mediaGrant(id, path).catch((error: unknown) => {
        if (open) refusals.set(path, error);
        throw error;
      });
      if (!open) {
        release(url);
        throw 'error.media.closed';
      }
      return url;
    },
    async grantWeb(address: string, eligible: () => boolean) {
      if (!open || !eligible()) return;
      const owner = await enter();
      if (!open || !eligible()) return;
      const url = await api.mediaGrantWeb(id, address);
      if (!open || !eligible()) {
        void api.mediaRelease(owner, url).catch(() => {});
        return;
      }
      if (!/^(?:faber-media:\/\/(?:app|localhost)|http:\/\/faber-media\.localhost)\/[a-f0-9]{32}$/.test(url)) {
        void api.mediaRelease(owner, url).catch(() => {});
        throw 'error.media.webUnreachable';
      }
      return url;
    },
    release,
    close() {
      open = false;
      stopWatching();
      elements.clear();
      closePopup?.();
      refusals.clear();
      void lease?.then((value) => api.mediaLeave(value)).catch(() => {});
    },
    async open(ref: MediaRef, origin: HTMLElement) {
      const request = ++requested;
      closePopup?.();
      if (ref.kind === 'picture') {
        const url = await session.grant(ref.path).catch(() => undefined);
        if (!open || request !== requested || !origin.isConnected) {
          if (url) release(url);
          return;
        }
        if (url) {
          const close = openViewer(origin, [{ src: url, alt: ref.name }], 0, () => {
            release(url);
            if (request === requested) closePopup = undefined;
          });
          closePopup = close;
          return;
        }
      }
      const dialog = document.createElement('dialog');
      dialog.className = 'reply-media-dialog';
      dialog.setAttribute('aria-label', ref.name);
      document.body.append(dialog);
      const dispose = render(() => <>
        <button class="iconbtn" aria-label={t('run.close')} onClick={() => dialog.close()}><Icon name="close" /></button>
        <ReplyMedia session={session} media={ref} onView={() => void session.open(ref, origin)} />
      </>, dialog);
      closePopup = () => {
        closePopup = undefined;
        dispose();
        dialog.remove();
        if (origin.isConnected) origin.focus({ preventScroll: true });
      };
      dialog.addEventListener('close', closePopup, { once: true });
      dialog.showModal();
    },
  };
  return session;
}

export type MediaSession = ReturnType<typeof mediaSession>;
type Session = MediaSession;
export const ReplyMediaContext = createContext<Accessor<Session>>();

export function ReplyMedia(props: { media: MediaRef; session?: Session; thumb?: boolean; tile?: boolean; onView?: (img: HTMLImageElement) => void; webAllowed?: () => boolean }) {
  const context = useContext(ReplyMediaContext);
  const scope = () => props.session ?? context?.();
  const [visible, setVisible] = createSignal(false);
  const [source, setSource] = createSignal<string>();
  const [status, setStatus] = createSignal<'loading' | 'ready' | 'error' | 'gone'>('loading');
  const [retry, setRetry] = createSignal(0);
  const [webFailure, setWebFailure] = createSignal<unknown>();
  let figure!: HTMLElement;
  let image: HTMLImageElement | undefined;
  let player: HTMLMediaElement | undefined;
  let retriedBusyPreview = false;

  createEffect(() => {
    const stop = scope()?.observe(props.thumb ? figure.closest('.step-line') ?? figure : figure, setVisible);
    onCleanup(() => {
      stop?.();
      setVisible(false);
    });
  });
  createEffect(() => {
    const session = scope();
    retry();
    const path = props.media.path;
    if (!visible() || !session) return;
    let current = true;
    let url: string | undefined;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    setStatus('loading');
    setWebFailure(undefined);
    const eligible = () => current && visible() && !document.hidden && figure.isConnected && props.webAllowed?.();
    const grantWeb = () => session.grantWeb(path, () => !!eligible());
    const accepted = (value: string | undefined) => {
      if (!value) return;
      if (!current || (props.webAllowed && !eligible())) return session.release(value);
      url = value;
      setSource(value);
    };
    const rejected = (error: unknown) => {
      if (!current || (props.webAllowed && !eligible())) return;
      if (props.tile && props.webAllowed && !retriedBusyPreview) {
        const { key, detail } = failureOf(error);
        const seconds = Number(detail);
        if (key === 'error.media.webBusy' && Number.isInteger(seconds) && seconds >= 1 && seconds <= 15) {
          retryTimer = setTimeout(() => {
            if (!eligible()) return;
            retriedBusyPreview = true;
            void grantWeb().then(accepted, rejected);
          }, seconds * 1000);
          return;
        }
      }
      if (props.webAllowed) setWebFailure(error);
      setStatus(refusal(error) ? 'gone' : 'error');
    };
    void (props.webAllowed ? grantWeb() : session.grant(path)).then(accepted, rejected);
    onCleanup(() => {
      current = false;
      clearTimeout(retryTimer);
      if (image) {
        image.removeAttribute('src');
        image = undefined;
      }
      if (player) {
        if (playing === player) pauseReplyMedia();
        else player.pause();
        player.removeAttribute('src');
        player.load();
        player = undefined;
      }
      setSource(undefined);
      if (url) session.release(url);
    });
  });
  const ready = () => setStatus('ready');
  const failed = () => setStatus('error');
  const play = (event: Event) => {
    const next = event.currentTarget as HTMLMediaElement;
    if (playing !== next) pauseReplyMedia();
    playing = next;
  };
  const playback = () => ({
    controls: true, preload: 'metadata', 'aria-label': props.media.name,
    onLoadedMetadata: ready,
    onError: failed, onPlay: play,
  } as const);
  if (props.thumb) return <span ref={(el) => { figure = el; }} class="reply-media-thumb" hidden={!source() || status() !== 'ready'} aria-hidden="true">
    <Show when={source()} keyed>{(url) => <img ref={(el) => { image = el; }} src={url} alt="" decoding="async" hidden={status() !== 'ready'} onLoad={ready} onError={failed} />}</Show>
  </span>;
  return <div ref={(el) => { figure = el; }} class="reply-media-shell" classList={{ 'reply-media-tile': props.tile }}>
    <figure class={`reply-media-${props.media.kind}`} aria-label={props.thumb ? undefined : props.media.name}>
      <div class="reply-media-slot">
        <Show when={source()} keyed>{(url) =>
          props.media.kind === 'picture' ?
            <button type="button" class="reply-media-picture-button" hidden={status() !== 'ready'}
              aria-label={t('viewer.open', { name: props.media.name })}
              onClick={(e) => {
                e.stopPropagation();
                const image = e.currentTarget.querySelector('img')!;
                if (props.onView) props.onView(image);
                else openViewer(image);
              }}>
              <img ref={(el) => { image = el; }} src={url} decoding="async" alt={props.media.name} data-view="" onLoad={ready} onError={failed} />
            </button> : props.media.kind === 'video' ?
              <video ref={(el) => { player = el; }} hidden={status() !== 'ready'} src={url} {...playback()} /> :
              <audio ref={(el) => { player = el; }} hidden={status() !== 'ready'} src={url} {...playback()} />
        }</Show>
        <Show when={!props.thumb && status() !== 'ready'}>
          <div class="reply-media-status" role={status() === 'loading' ? undefined : 'status'}>
            <span class="reply-media-mark" data-status={status()} aria-hidden="true">
              <Show when={status() !== 'loading'}><Icon name={status() === 'gone' ? 'sheet' : 'close'} /></Show>
            </span>
            <span>{webFailure() ? failureText(webFailure()) : t(status() === 'gone' ? 'media.gone' : status() === 'error' ? 'media.error' : 'file.loading')}</span>
            <Show when={status() === 'error'}><button type="button" class="btn ghost" onClick={() => {
              scope()?.retry(props.media.path);
              setRetry((n) => n + 1);
            }}>{t('changes.retry')}</button></Show>
          </div>
        </Show>
      </div>
      <Show when={!props.tile || (props.webAllowed && (status() === 'error' || status() === 'gone'))}>
        <figcaption>
          <Show when={props.webAllowed} fallback={<span>{props.media.name}</span>}>
            <a href={props.media.path}>{props.media.name}</a>
          </Show>
          <Show when={props.media.kind === 'audio'}><small>{(props.webAllowed ? new URL(props.media.path).pathname : props.media.path).split('.').at(-1)?.toUpperCase()}</small></Show>
        </figcaption>
      </Show>
    </figure>
  </div>;
}

export function ReplyGallery(props: { media: MediaRef[]; session?: Session; webAllowed?: (ref: MediaRef) => boolean }) {
  const context = useContext(ReplyMediaContext);
  const previews = props.media.slice(0, 4);
  let close: (() => void) | undefined;
  onCleanup(() => close?.());
  const view = (origin: HTMLElement, index: number) => {
    const session = props.session ?? context?.();
    if (!session) return;
    let active = true;
    const shots = props.media.map((ref) => ({
      alt: ref.name,
      href: props.webAllowed ? ref.path : undefined,
      load: async () => {
        const src = props.webAllowed
          ? await session.grantWeb(ref.path, () => active && origin.isConnected && !document.hidden && !!props.webAllowed?.(ref))
          : await session.grant(ref.path);
        if (!src) throw 'error.media.closed';
        return { src, release: () => session.release(src) };
      },
    }));
    close = openViewer(origin, shots, index, () => { active = false; }) ?? close;
  };
  return <div class="reply-media-gallery" style={{ '--gallery-columns': previews.length }}>
    <For each={previews}>{(media, i) =>
      <ReplyMedia media={media} session={props.session} tile
        webAllowed={props.webAllowed && (() => !!props.webAllowed?.(media))} onView={(img) => view(img, i())} />
    }</For>
    <Show when={props.media.length > previews.length}>
      <button type="button" class="btn ghost reply-gallery-more" onClick={(event) => view(event.currentTarget, previews.length)}>
        {t('media.more', { n: props.media.length - previews.length })}
      </button>
    </Show>
  </div>;
}
