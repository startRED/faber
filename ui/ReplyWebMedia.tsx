import { Match, Show, Switch, createSignal } from 'solid-js';
import type { WebSearch } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { failureText, t } from './i18n.ts';
import { mediaKind, ReplyMedia, type MediaSession } from './Media.tsx';

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
  const reason = () => {
    if (props.mode === 'off') return t('error.media.webOff');
    if (props.mode === 'cached') return t('error.media.webCached');
    if (blocked()) return t('error.media.webBlocked');
    if (props.mode !== 'live') return t('media.webUnavailable');
    if (!props.media.url.startsWith('https://')) return t('error.media.webHttps');
    if (!kind) return t('error.media.webKind');
  };
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
