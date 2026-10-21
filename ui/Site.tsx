import { type Accessor, createEffect, createResource, createSignal, For, Index, on, onCleanup, onMount, Show } from 'solid-js';
import { api, type Message, type ToolUse } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

/** A page the agent opened in Faber's browser (R161 step 7), which the person opened from the reply. */
export interface OpenSite {
  conversationId: string;
  url: string;
  title: string;
  /** What the agent did in the browser during the reply, oldest first, kept up to date while it works (step 8, "recorded"). */
  steps?: Accessor<SiteStep[]>;
}

/** One thing the agent did in Faber's browser, and the picture the browser took after it. */
export interface SiteStep {
  call: string;
  words: string;
  /** The screen sizes a check looked at the page on, a picture each, in order. */
  looks?: SiteLook[];
}

/** The page at one screen size, in a check at several (`engine/src/browser.rs`, `looks_text`). */
export interface SiteLook {
  /** The picture's call id: the call's own with the size's place after it. */
  picture: string;
  screen: 'site.screen.pc' | 'site.screen.mac' | 'site.screen.tablet' | 'site.screen.phone';
  width: number;
  height: number;
  fits: boolean;
}

const SCREENS = { 'a PC': 'site.screen.pc', 'a MacBook': 'site.screen.mac', 'a tablet': 'site.screen.tablet', 'a phone': 'site.screen.phone' } as const;
const SIZE = /^(a PC|a MacBook|a tablet|a phone), (\d+)x(\d+) at [\d.]+x/;

/** Faber's own lines of an answer, above the page's words. */
const headOf = (tool: ToolUse) => (tool.output ?? '').split("\nThe page's visible text follows.", 1)[0]!;

/** The sizes a check looked at, read from Faber's own lines ("Size 2 of 6: a phone, 390x844 at 3x: fits."). */
export function looksOf(tool: ToolUse): SiteLook[] {
  if (tool.name !== 'Browser' || tool.ok !== true) return [];
  return headOf(tool)
    .split('\n')
    .flatMap((line) => {
      const [, place, rest] = /^Size (\d+) of \d+: (.*)$/.exec(line) ?? [];
      const [said, screen, width, height] = SIZE.exec(rest ?? '') ?? [];
      if (!said) return [];
      // Only Faber's words for a page wider than the screen, or cut by its edge, say it does not fit; a size the person's turn
      // stopped, or one with no picture, says nothing of it.
      const fits = !/^: (?:\d+ px wider than the screen|cut by the right edge)/.test(rest!.slice(said.length));
      return [{ picture: `${tool.id}-s${place}`, screen: SCREENS[screen as keyof typeof SCREENS], width: Number(width), height: Number(height), fits }];
    });
}

/** The size a page is shown at, from Faber's own line, when the agent chose one. */
function sizeOf(tool: ToolUse): string | undefined {
  const [said, screen, width, height] = /^Shown at the size of (a PC|a MacBook|a tablet|a phone), (\d+)x(\d+)/m.exec(headOf(tool)) ?? [];
  return said ? t('site.size', { screen: t(SCREENS[screen as keyof typeof SCREENS]), width: width!, height: height! }) : undefined;
}

const quoted = (text: unknown) => (typeof text === 'string' ? text.replace(/\s+/g, ' ').trim() : '');

/** A browser call in plain words. What the agent typed never shows, only where: it can be a search, a name or a secret. */
export function stepWords(tool: ToolUse): string {
  const input = (tool.input ?? {}) as { url?: unknown; action?: unknown; target?: unknown; up?: unknown; size?: unknown };
  if (typeof input.url === 'string') return t('site.step.open', { host: hostOf(siteOf(tool)?.url ?? input.url) });
  const target = quoted(input.target);
  if (input.size !== undefined && input.size !== null) {
    const size = sizeOf(tool);
    return size ? t('site.step.size', { size }) : t('site.step.ownSize');
  }
  switch (input.action) {
    case 'sizes':
      return t('site.step.sizes', { n: looksOf(tool).length });
    case 'click':
      return t('site.step.click', { target });
    case 'type':
      return target ? t('site.step.type', { target }) : t('site.step.typeHere');
    case 'scroll':
      return t(input.up === true ? 'site.step.up' : 'site.step.down');
    case 'back':
      return t('site.step.back');
    default:
      return t('site.step.read');
  }
}

/** Every call of a reply that the browser did, in order: the log the page view steps through. */
export const stepsOf = (tools: ToolUse[]): SiteStep[] =>
  tools
    .filter((tool) => tool.name === 'Browser' && tool.ok === true)
    .map((tool) => {
      const looks = looksOf(tool);
      return { call: tool.id, words: stepWords(tool), ...(looks.length ? { looks } : {}) };
    });

/** What "While you were away" shows of the browser (step 8): the latest step on each page the run left over its last `turns`
 * replies, the newest four, each with the picture taken after it. */
export function awaySites(messages: Message[], turns: number): SiteStep[] {
  const replies = messages.filter((m) => m.role === 'assistant').slice(-Math.max(1, turns));
  const tools = replies.flatMap((m) => (m.tools ?? []).filter((tool) => !tool.parent));
  return sitesOf(tools).slice(-4).map(({ tool }) => ({ call: tool.id, words: stepWords(tool) }));
}

/** One page of "While you were away": its picture, when one was kept, and what the agent last did there. */
export function AwayPage(props: { conversationId: string; step: SiteStep }) {
  const [picture] = createResource(() => props.step.call, (call) => api.sitePicture(props.conversationId, call).catch(() => null));
  return (
    <li>
      <Show when={picture()} fallback={<span class="away-none" />}>
        {(src) => <img src={src()} alt="" />}
      </Show>
      <span title={props.step.words}>{props.step.words}</span>
    </li>
  );
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

// The colour codes a dev server wraps its lines in (ESC [ … m), an address on this computer up to the next space or quote,
// the prose that can close a sentence after it, and the words outside the addresses that say a server is up or is not.
const COLOURS = new RegExp(String.raw`${String.fromCharCode(27)}\[[0-9;]*[A-Za-z]`, 'g');
const LOCAL = /\bhttps?:\/\/(?:localhost|127\.0\.0\.1|\[::1\]):\d+[^\s'"`<>]*/gi;
const CLOSING = /[.,;:!?)\]}]+$/;
const STARTED = /\b(?:local|listening|running|ready|started|starting|serving|available)\b/i;
const FAILED = /\b(?:fail\w*|error|refused|cannot|can't|unable|could not|not|no longer|waiting|ECONNREFUSED)\b/i;
const ALONE = /^\s*(?:[-*•➜]\s*)?https?:\/\/\S+\s*$/;

/** The local addresses with a port on one line. */
function addresses(line: string): string[] {
  return [...line.matchAll(LOCAL)].flatMap(([found]) => {
    try {
      const url = new URL(found.replace(CLOSING, ''));
      return url.port ? [url.href] : [];
    } catch {
      return [];
    }
  });
}

/** The local addresses an output announces: on a line whose own words say a server is up there and name no failure, or
 * listed one per line right under such a line that names none (http-server's "Available on:"). */
function announced(output: string): string[] {
  const out: string[] = [];
  let listing = false;
  for (const line of output.replace(COLOURS, '').split('\n')) {
    const words = line.replace(LOCAL, ' ');
    const found = addresses(line);
    if (listing && ALONE.test(line)) {
      out.push(...found);
      continue;
    }
    const up = STARTED.test(words) && !FAILED.test(words);
    if (up) out.push(...found);
    listing = up && found.length === 0;
  }
  return out;
}

/** The apps the agent started on this computer (step 7, the app pane): a local address with its port that a command's
 * output announces, such as a dev server's "Local: http://localhost:3000/", never one an error or a page of text only
 * mentions; one card per server, the latest command's address. A server the agent already opened in Faber's browser has
 * its page card instead. */
export function appsOf(tools: ToolUse[]): { tool: ToolUse; site: { url: string; title: string } }[] {
  const opened = new Set(sitesOf(tools).map(({ site }) => hostOf(site.url)));
  const out = new Map<string, { tool: ToolUse; site: { url: string; title: string } }>();
  for (const tool of tools) {
    if (!/bash|command|exec|shell/i.test(tool.name) || !tool.output) continue;
    for (const url of announced(tool.output)) {
      const host = hostOf(url);
      if (opened.has(host)) continue;
      out.delete(host);
      out.set(host, { tool, site: { url, title: t('site.app') } });
    }
  }
  return [...out.values()];
}

/** Where a page lives, with its port when it has one: the person's own servers differ only by it. */
export const hostOf = (url: string) => {
  try {
    return new URL(url).host.replace(/^www\./, '');
  } catch {
    return url;
  }
};

/** A page the agent opened, in its reply: the picture it took, the title and the site, and a button to look at it. */
export function SiteCard(props: { conversationId: string; tool: ToolUse; site: { url: string; title: string; turn?: SiteTurn }; steps?: Accessor<SiteStep[]> }) {
  const [picture] = createResource(() => props.tool.id, (call) => api.sitePicture(props.conversationId, call).catch(() => null));
  const open = async () => {
    const { url, title } = props.site;
    if (await api.siteOpen(props.conversationId, url).catch(() => false)) showSite({ conversationId: props.conversationId, url, title, steps: props.steps });
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

/** A check at several screen sizes, in the page's place: each size's picture with its name, and where the page runs past the
 * screen's edge, said in words. The pictures load only while the step is picked. */
function SiteLooks(props: { conversationId: string; looks: SiteLook[] }) {
  return (
    <ul class="site-looks" aria-label={t('site.sizes')}>
      <Index each={props.looks}>
        {(look) => {
          const [picture] = createResource(() => look().picture, (call) => api.sitePicture(props.conversationId, call).catch(() => null));
          const size = () => t('site.size', { screen: t(look().screen), width: look().width, height: look().height });
          return (
            <li>
              <Show when={picture()} fallback={<span class="site-look-none" />}>
                {(src) => <img src={src()} alt={size()} style={{ 'aspect-ratio': `${look().width} / ${look().height}` }} />}
              </Show>
              <span>{size()}</span>
              <Show when={!look().fits}>
                <span class="site-look-wider">{t('site.size.wider')}</span>
              </Show>
            </li>
          );
        }}
      </Index>
    </ul>
  );
}

/** The person's steps in the pane's head, in the order a browser puts them. */
const STEPS = [
  ['back', 'left', 'site.back'],
  ['forward', 'right', 'site.forward'],
  ['reload', 'retry', 'site.reload'],
] as const;

/** What the person typed in the address field, as an address the browser loads: a bare name gets https. The shell still
 * refuses anything but a web page (`allowed` in src-tauri/src/pane/mod.rs). */
export function addressOf(typed: string): string | undefined {
  const text = typed.trim();
  if (!text || /\s/.test(text)) return undefined;
  // "localhost:3000" names a host and a port, not a scheme; an app on this computer usually serves plain http.
  const schemed = /^[a-z][a-z0-9+.-]*:/i.test(text) && !/^[^:/]+:\d+(\/|$)/.test(text);
  const url = schemed ? text : /^(localhost|127\.0\.0\.1)([:/]|$)/i.test(text) ? `http://${text}` : `https://${text}`;
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:' ? parsed.href : undefined;
  } catch {
    return undefined;
  }
}

/** The page in the right column: Faber's browser sits over this box, measured here, and leaves when the box goes. Every
 * move of the box moves it, and it is placed again whenever the window changes size. The address field above it takes
 * the person anywhere on the web; with no page yet, only the field shows. */
export function SitePane(props: { site: OpenSite; onClose: () => void }) {
  let box: HTMLDivElement | undefined;
  // The shell never places a box again once it let the page go, so coming back from a step of the log takes a new one.
  let id = boxId();
  // Where the page is now: a link or a redirect moves it off the card's site, and the head then names the new one.
  const [live, setLive] = createSignal(props.site.url);
  const stop = api.onSite(setLive);
  onCleanup(() => void stop.then((unlisten) => unlisten()).catch(() => {}));
  const place = () => {
    // While a step of the log shows, its picture holds the box and the page stays out of the window; with no page yet there
    // is nothing to place.
    if (picked() || blank()) return;
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
  // A step picked in the log shows the picture taken after it, in the page's place; none picked is the live page.
  const steps = () => props.site.steps?.() ?? [];
  const [picked, setPicked] = createSignal<string>();
  const pickedAt = () => steps().findIndex((s) => s.call === picked());
  const [still] = createResource(picked, (call) => api.sitePicture(props.site.conversationId, call).catch(() => null));
  // The live page goes out of the window while a picture stands in for it, and comes back when the log lets go.
  createEffect(
    on(
      picked,
      (call) => {
        if (call) return void api.sitePark(id);
        id = boxId();
        place();
      },
      { defer: true },
    ),
  );
  // Another card, or an address typed in the field, starts on its live page, placed at once (the first one after a blank pane).
  createEffect(on(() => props.site, () => (setPicked(undefined), place()), { defer: true }));
  // The latest step is the live page, unless it is a check at several sizes: its pictures are what it saw.
  const pick = (call?: string) => setPicked(call === steps().at(-1)?.call && !steps().at(-1)?.looks ? undefined : call);
  const keys = (e: KeyboardEvent) => {
    const list = steps();
    // From the row that has the focus: the agent may have added steps since it got it.
    const buttons = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('button')];
    const focused = buttons.indexOf(document.activeElement as HTMLElement);
    const at = focused >= 0 ? focused : picked() === undefined ? list.length - 1 : pickedAt();
    const to = e.key === 'ArrowUp' || e.key === 'ArrowLeft' ? at - 1 : e.key === 'ArrowDown' || e.key === 'ArrowRight' ? at + 1 : e.key === 'Home' ? 0 : e.key === 'End' ? list.length - 1 : -2;
    if (to === -2 || to < 0 || to >= list.length) return;
    e.preventDefault();
    pick(list[to]!.call);
    buttons[to]?.focus();
  };
  const moved = () => hostOf(live()) !== hostOf(props.site.url);
  const blank = () => !props.site.url;
  const title = () => (blank() ? t('site.browser') : moved() ? hostOf(live()) : props.site.title || hostOf(props.site.url));
  // The address field shows where the page is until the person types; Enter loads what they typed, Esc puts it back.
  const [typed, setTyped] = createSignal<string>();
  const [refused, setRefused] = createSignal(false);
  createEffect(on(live, () => setTyped(undefined), { defer: true }));
  const go = async (to: string | undefined) => {
    setRefused(false);
    if (!to || !(await api.siteOpen(props.site.conversationId, to).catch(() => false))) return setRefused(true);
    setTyped(undefined);
    // The new page is the live one, even on the same site: a step's picture that stood in for the page goes.
    setPicked(undefined);
    setLive(to);
    if (blank() || hostOf(to) !== hostOf(props.site.url)) showSite({ conversationId: props.site.conversationId, url: to, title: '' });
  };
  const addressKeys = (e: KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      void go(addressOf(typed() ?? live()));
    } else if (e.key === 'Escape' && typed() !== undefined) {
      // Only the typing goes: the column itself stays.
      e.stopPropagation();
      setTyped(undefined);
      setRefused(false);
    }
  };
  return (
    <>
      <div class="rc-head side-head">
        <h2 id="running-title" class="display" title={live()}>
          {title()}
        </h2>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
          <Icon name="close" />
        </button>
      </div>
      <div class="site-bar">
        {/* Only the live page steps: while a step's picture stands in for it, the page is out of sight. */}
        <Show when={!blank() && !picked()}>
          <For each={STEPS}>
            {([step, icon, label]) => (
              <button class="iconbtn" aria-label={t(label)} title={t(label)} onClick={() => void api.siteStep(props.site.conversationId, step).catch(() => false)}>
                <Icon name={icon} />
              </button>
            )}
          </For>
        </Show>
        <input
          class="field site-address"
          type="text"
          inputmode="url"
          spellcheck={false}
          autocomplete="off"
          aria-label={t('site.address')}
          aria-invalid={refused() || undefined}
          placeholder={t('site.addressHint')}
          value={typed() ?? (blank() ? '' : live())}
          onInput={(e) => (setTyped(e.currentTarget.value), setRefused(false))}
          onFocus={(e) => e.currentTarget.select()}
          onKeyDown={addressKeys}
        />
      </div>
      <Show when={refused()}>
        <p class="side-meta bad" role="alert">{t('site.refused')}</p>
      </Show>
      <div class="rc-body side-body site-body">
        <p class="side-meta">{blank() ? t('site.blank') : t('site.meta', { host: hostOf(live()) })}</p>
        {/* The sizes of a check are a list a screen reader walks, never one image. */}
        <div
          class="site-view"
          classList={{ blank: blank() }}
          ref={(el) => (box = el)}
          role={picked() && steps()[pickedAt()]?.looks ? undefined : 'img'}
          aria-label={picked() && steps()[pickedAt()]?.looks ? undefined : t('site.view', { title: title() })}
        >
          <Show when={picked()}>
            <Show
              when={steps()[pickedAt()]?.looks}
              fallback={
                <Show when={still()} fallback={<Show when={!still.loading}><p class="site-none">{t('site.step.noPicture')}</p></Show>}>
                  {(src) => <img src={src()} alt="" />}
                </Show>
              }
            >
              {(looks) => <SiteLooks conversationId={props.site.conversationId} looks={looks()} />}
            </Show>
          </Show>
        </div>
        <Show when={steps().length > 1 || steps().some((step) => step.looks)}>
          <div class="site-log">
            <Show when={picked()}>
              <button class="btn" onClick={() => setPicked(undefined)}>
                {t('site.live')}
              </button>
            </Show>
            <ol aria-label={t('site.steps')} onKeyDown={keys}>
              <Index each={steps()}>
                {(step, i) => {
                  // By place, not by object: a new step of the agent keeps the rows, and the focus, that are there.
                  const on = () => (picked() === undefined ? i === steps().length - 1 : picked() === step().call);
                  return (
                    <li>
                      <button aria-current={on() ? 'step' : undefined} tabIndex={on() ? 0 : -1} title={step().words} onClick={() => pick(step().call)}>
                        <span>{i + 1}</span>
                        {step().words}
                      </button>
                    </li>
                  );
                }}
              </Index>
            </ol>
          </div>
        </Show>
      </div>
    </>
  );
}
