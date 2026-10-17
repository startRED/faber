import { createResource, createSignal, onCleanup, onMount, Show } from 'solid-js';
import { api, type ToolUse } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

/** A page the agent opened in Faber's browser (R161 step 7), which the person opened from the reply. */
export interface OpenSite {
  conversationId: string;
  url: string;
  title: string;
}

const [opened, setOpened] = createSignal<OpenSite>();
/** The page open in the running column, if any. */
export const openedSite = opened;
/** Opens a page in the running column, or goes back to the running list. */
export const showSite = (site?: OpenSite) => setOpened(site);

/** What a browser call opened: the address the page ended on and its title, read from Faber's own first lines of the answer
 * (the page's words come after them), else the address the agent asked for. */
export function siteOf(tool: ToolUse): { url: string; title: string } | undefined {
  if (tool.name !== 'Browser' || tool.ok !== true) return undefined;
  const asked = (tool.input as { url?: unknown } | undefined)?.url;
  const head = tool.output?.split('\n', 2) ?? [];
  const ours = /^Opened (https?:\/\/\S+) in Faber's browser/.exec(head[0] ?? '')?.[1];
  const url = ours ?? (typeof asked === 'string' ? asked : '');
  if (!/^https?:\/\//.test(url)) return undefined;
  // The title line is Faber's only right under its own first line.
  const title = ours ? /^Title: (.*)$/.exec(head[1] ?? '')?.[1]?.trim() : undefined;
  return { url, title: title ?? '' };
}

export const hostOf = (url: string) => {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
};

/** A page the agent opened, in its reply: the picture it took, the title and the site, and a button to look at it. */
export function SiteCard(props: { conversationId: string; tool: ToolUse; site: { url: string; title: string } }) {
  const [picture] = createResource(() => props.tool.id, (call) => api.sitePicture(props.conversationId, call).catch(() => null));
  const open = async () => {
    if (await api.siteOpen(props.site.url).catch(() => false)) showSite({ conversationId: props.conversationId, ...props.site });
  };
  return (
    <div class="file site">
      <Show when={picture()}>{(src) => <img src={src()} alt="" />}</Show>
      <div>
        <span title={props.site.url}>
          <b>{props.site.title || hostOf(props.site.url)}</b>
          <Show when={props.site.title}>
            <small>{hostOf(props.site.url)}</small>
          </Show>
        </span>
        <button class="btn" onClick={() => void open()}>
          {t('site.show')}
        </button>
      </div>
    </div>
  );
}

/** The page in the running column: Faber's browser sits over this box, measured here, and leaves when the box goes. Every
 * move of the box moves it, and it is placed again whenever the window changes size. */
export function SitePane(props: { site: OpenSite; onBack: () => void; onClose: () => void }) {
  let box: HTMLDivElement | undefined;
  // Where the page is now: a link or a redirect moves it off the card's site, and the head then names the new one.
  const [live, setLive] = createSignal(props.site.url);
  const stop = api.onSite(setLive);
  onCleanup(() => void stop.then((unlisten) => unlisten()).catch(() => {}));
  const place = () => {
    const r = box?.getBoundingClientRect();
    if (!r) return;
    if (r.width >= 1 && r.height >= 1) void api.sitePlace(r.left, r.top, r.width, r.height);
  };
  onMount(() => {
    const seen = new ResizeObserver(place);
    seen.observe(box!);
    window.addEventListener('resize', place);
    // The column slides in: the box is placed again once it has arrived.
    const column = box!.closest('.runcol');
    column?.addEventListener('transitionend', place);
    onCleanup(() => {
      seen.disconnect();
      window.removeEventListener('resize', place);
      column?.removeEventListener('transitionend', place);
      void api.sitePark();
    });
  });
  const moved = () => hostOf(live()) !== hostOf(props.site.url);
  const title = () => (moved() ? hostOf(live()) : props.site.title || hostOf(props.site.url));
  return (
    <>
      <div class="rc-head side-head">
        <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={() => props.onBack()}>
          <Icon name="left" />
        </button>
        <h2 id="running-title" class="display" title={live()}>
          {title()}
        </h2>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
          <Icon name="close" />
        </button>
      </div>
      <div class="rc-body side-body site-body">
        <p class="side-meta">{t('site.meta', { host: hostOf(live()) })}</p>
        <div class="site-view" ref={(el) => (box = el)} role="img" aria-label={t('site.view', { title: title() })} />
      </div>
    </>
  );
}
