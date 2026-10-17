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
// Each box the page is placed over is named by a number that only grows while the screen lives (the shell forgets them all
// when a screen loads, see main.tsx), so a placing that arrives after its box let the page go is ignored by the shell.
let boxes = 0;
const boxId = () => ++boxes;
/** The page open in the running column, if any. */
export const openedSite = opened;
/** Opens a page in the running column, or goes back to the running list. */
export const showSite = (site?: OpenSite) => setOpened(site);

/** What on a page only the person does (`engine/src/browser.rs`, `Turn`), and the words the card says it with. */
const TURNS = { 'a login': 'site.turn.login', 'a captcha': 'site.turn.captcha', 'a payment': 'site.turn.payment' } as const;
export type SiteTurn = (typeof TURNS)[keyof typeof TURNS];

/** What a browser call left open: the address the page ended on, its title and whether it is the person's turn there, read
 * from Faber's own first lines of the answer (the page's words come after them), else the address the agent asked for. */
export function siteOf(tool: ToolUse): { url: string; title: string; turn?: SiteTurn } | undefined {
  if (tool.name !== 'Browser' || tool.ok !== true) return undefined;
  const asked = (tool.input as { url?: unknown } | undefined)?.url;
  const head = tool.output?.split('\n', 3) ?? [];
  const ours = /^Opened (https?:\/\/\S+) in Faber's browser/.exec(head[0] ?? '')?.[1];
  const url = ours ?? (typeof asked === 'string' ? asked : '');
  if (!/^https?:\/\//.test(url)) return undefined;
  // The title line is Faber's only right under its own first line.
  const title = ours ? /^Title: (.*)$/.exec(head[1] ?? '')?.[1]?.trim() : undefined;
  const turn = ours ? /^Your turn: the page asks for (a login|a captcha|a payment),/.exec(head[2] ?? '')?.[1] : undefined;
  return { url, title: title ?? '', ...(turn ? { turn: TURNS[turn as keyof typeof TURNS] } : {}) };
}

/** The pages a reply's calls left open, one card each: a page the agent clicked through several times is the latest of them. */
export function sitesOf(tools: ToolUse[]): { tool: ToolUse; site: NonNullable<ReturnType<typeof siteOf>> }[] {
  const out = new Map<string, { tool: ToolUse; site: NonNullable<ReturnType<typeof siteOf>> }>();
  for (const tool of tools) {
    const site = siteOf(tool);
    if (!site) continue;
    out.delete(site.url);
    out.set(site.url, { tool, site });
  }
  return [...out.values()];
}

export const hostOf = (url: string) => {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
};

/** A page the agent opened, in its reply: the picture it took, the title and the site, and a button to look at it. */
export function SiteCard(props: { conversationId: string; tool: ToolUse; site: { url: string; title: string; turn?: SiteTurn } }) {
  const [picture] = createResource(() => props.tool.id, (call) => api.sitePicture(props.conversationId, call).catch(() => null));
  const open = async () => {
    const { url, title } = props.site;
    if (await api.siteOpen(props.conversationId, url).catch(() => false)) showSite({ conversationId: props.conversationId, url, title });
  };
  return (
    <div class="file site" data-turn={props.site.turn ? 'person' : undefined}>
      <Show when={picture()}>{(src) => <img src={src()} alt="" />}</Show>
      <Show when={props.site.turn}>{(turn) => <p class="site-turn">{t(turn())}</p>}</Show>
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
  const id = boxId();
  // Where the page is now: a link or a redirect moves it off the card's site, and the head then names the new one.
  const [live, setLive] = createSignal(props.site.url);
  const stop = api.onSite(setLive);
  onCleanup(() => void stop.then((unlisten) => unlisten()).catch(() => {}));
  const place = () => {
    const r = box?.getBoundingClientRect();
    if (!r) return;
    if (r.width >= 1 && r.height >= 1) void api.sitePlace(id, r.left, r.top, r.width, r.height);
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
      void api.sitePark(id);
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
