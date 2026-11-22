import { For, Match, Show, Switch, createMemo, createSignal } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import type { WebSearch } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { failureText, t } from './i18n.ts';
import { mediaKind, ReplyGallery, ReplyMedia, type MediaRef, type MediaSession } from './Media.tsx';

const videoSites = new Map([
  ['youtube.com', 'YouTube'], ['m.youtube.com', 'YouTube'], ['youtu.be', 'YouTube'],
  ['vimeo.com', 'Vimeo'], ['player.vimeo.com', 'Vimeo'],
]);

export interface WebReply {
  url: string;
  title: string;
  host: string;
  picture: boolean;
  site?: string;
}

export interface ReplyWebOptions {
  mode?: WebSearch;
  blocked: string[];
  canOpen: boolean;
  open: (url: string, title: string) => Promise<boolean | undefined>;
  onError?: (reason: unknown) => void;
  onSettings?: () => void;
}

export function replyAddress(address: string): URL | undefined {
  if (!/^https?:\/\//i.test(address) || /\s/.test(address)) return;
  try {
    const url = new URL(address);
    if (url.hostname && !url.username && !url.password) return url;
  } catch { return; }
}

export function webReply(address: string, title: string, picture: boolean): WebReply | undefined {
  const url = replyAddress(address);
  if (!url) return;
  const host = url.hostname.replace(/\.$/, '');
  const site = !picture && videoSites.get(host.replace(/^www\./, '')) || undefined;
  if (!site && !picture && !mediaKind(url.pathname)) return;
  return { url: url.href, title: title.trim() || url.href, host, picture, site };
}

export function replyHostBlocked(host: string, blocked: string[]) {
  return blocked.some((entry) => host === entry || host.endsWith('.' + entry));
}

export function isWebPicture(media: WebReply) {
  return !media.site && (media.picture || mediaKind(new URL(media.url).pathname) === 'picture');
}

function webReason(media: WebReply, options: ReplyWebOptions, kind: MediaRef['kind'] | undefined) {
  if (options.mode === 'off') return t('error.media.webOff');
  if (options.mode === 'cached') return t('error.media.webCached');
  if (replyHostBlocked(media.host, options.blocked)) return t('error.media.webBlocked');
  if (options.mode !== 'live') return t('media.webUnavailable');
  if (!media.url.startsWith('https://')) return t('error.media.webHttps');
  if (!kind) return t('error.media.webKind');
}

export function ReplyWebPictures(props: {
  pictures: WebReply[]; list?: { ordered: boolean; start: number }; session: MediaSession;
} & ReplyWebOptions) {
  const available = createMemo(() => props.pictures.filter(picture => !webReason(picture, props, 'picture')));
  const gallery = createMemo<MediaRef[]>(() => available().map(picture => ({
    path: picture.url, name: picture.title, kind: 'picture',
  })));
  const renderPicture = (picture: WebReply) => <ReplyWebMedia {...props} media={picture} />;
  return <Show when={gallery().length > 1 ? gallery() : undefined} keyed fallback={
    props.list
      ? <Dynamic component={props.list.ordered ? 'ol' : 'ul'} start={props.list.ordered ? props.list.start : undefined} data-reply-picture-list="">
        <For each={props.pictures}>{picture => <li>{renderPicture(picture)}</li>}</For>
      </Dynamic>
      : <For each={props.pictures}>{renderPicture}</For>
  }>{(pictures) => <>
    <ReplyGallery media={pictures} session={props.session}
      webAllowed={ref => available().some(picture => picture.url === ref.path)} />
    <For each={props.pictures.filter(picture => !available().includes(picture))}>{renderPicture}</For>
  </>}</Show>;
}

export function ReplyWebMedia(props: { media: WebReply; session: MediaSession } & ReplyWebOptions) {
  const [opening, setOpening] = createSignal(false);
  const [refused, setRefused] = createSignal<string>();
  const untitledSite = () => replyAddress(props.media.title)?.href === props.media.url ? props.media.site : undefined;
  const title = () => {
    const site = untitledSite();
    return site ? t('media.webTitle', { site }) : props.media.title;
  };
  const play = async () => {
    if (opening() || blocked()) return;
    setRefused(undefined);
    setOpening(true);
    try { if (await props.open(props.media.url, title()) === false) setRefused(t('media.webPaneUnavailable')); }
    catch (err) { setRefused(failureText(err)); }
    finally { setOpening(false); }
  };
  const blocked = () => replyHostBlocked(props.media.host, props.blocked);
  const kind = props.media.picture ? 'picture' : mediaKind(new URL(props.media.url).pathname);
  const reason = () => webReason(props.media, props, kind);
  return <Switch>
    <Match when={props.media.site && !props.canOpen}>
      <p><a href={props.media.url}>{props.media.title}</a></p>
    </Match>
    <Match when={props.media.site}>
      {(site) => <div class="reply-web-video">
        <button type="button" class="reply-web-play" aria-label={t('media.webPlay', { name: title() })}
          aria-description={untitledSite() ? props.media.url : undefined} disabled={opening() || blocked()} onClick={() => void play()}>
          <Icon name="play" />
        </button>
        <div>
          <a href={props.media.url} title={untitledSite() ? props.media.url : undefined}>{title()}</a>
          <small><Show when={!untitledSite()}>{site()} · </Show>{blocked() ? t('error.browser.blockedHost') : t('media.webSide')}</small>
          <Show when={!blocked() && refused()}><small role="status">{refused()}</small></Show>
        </div>
      </div>}
    </Match>
    <Match when={props.mode === 'off'}>
      <a data-reply-web-off="" href={props.media.url}>{props.media.title}</a>
    </Match>
    <Match when={!props.media.site}>
      <Show when={!reason()} fallback={<div class="reply-web-notice">
        <a href={props.media.url}>{props.media.title}</a><span class="reply-web-reason"><Icon name="globe" small />{reason()}</span>
      </div>}>
        <ReplyMedia media={{ path: props.media.url, name: props.media.title, kind: kind! }}
          session={props.session} webAllowed={() => !reason()} />
      </Show>
    </Match>
  </Switch>;
}
