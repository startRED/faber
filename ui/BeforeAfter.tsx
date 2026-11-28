import { createContext, createEffect, createMemo, createResource, createSignal, createUniqueId, For, on, onCleanup, Show, untrack, useContext, type Accessor } from 'solid-js';
import { api, type AnswerPage, type Message } from './api.ts';
import { ReplyMediaContext } from './Media.tsx';
import { t } from './i18n.ts';
import { openViewer } from './Viewer.tsx';

// The pages of the person's app before and after a project answer, as Faber pictured them (`engine/src/engine/pictures.rs`):
// one quiet row of small thumbnails under the answer, before then after for each page, that opens the picture viewer.
// Nothing shows for an answer with no page pictured on both sides.

/** The gaps between the thread's reads again while its newest answer waits for an after picture: the engine takes them once
 * the answer ends, up to three pages at eight seconds each, so the reads stop about 30 seconds after it. */
const AGAIN_MS = [3000, 3000, 4000, 5000, 6000, 9000];

type Side = 'before' | 'after';
const SIDES: readonly Side[] = ['before', 'after'];
type Pair = { page: number; before: string; after: string };
type Pictured = Record<string, AnswerPage[]>;

/** Every pictured answer of the thread on screen, by message id (`answerPicturesOf`). */
export const AnswerPictures = createContext<Accessor<Pictured | undefined>>(() => undefined);

/** One read per project thread: when it opens and each time its newest answer finishes, never per answer. While that answer
 * has a page with a before picture and no after one yet, the whole thread is read again, at growing gaps, then no more.
 * The map, its reads again and their timer belong to one conversation: another one on screen starts them over. */
export function answerPicturesOf(props: { conversationId: string; area?: 'chat' | 'project'; messages: Message[] }): Accessor<Pictured | undefined> {
  const newest = createMemo(() => (props.area === 'project' ? props.messages.findLast((m) => m.role === 'assistant' && m.status !== 'streaming')?.id : undefined));
  const key = createMemo(() => { const id = newest(); return id ? `${props.conversationId}|${id}` : false; });
  const [pictured, { refetch }] = createResource(key, async (at) => {
    const conversation = at.split('|')[0]!;
    return { conversation, map: await api.answerPictures(conversation).catch((): Pictured => ({})) };
  });
  let asked = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  createEffect(on(key, () => { asked = 0; clearTimeout(timer); timer = undefined; }));
  createEffect(() => {
    const now = pictured();
    const id = newest();
    if (pictured.loading || !id || now?.conversation !== props.conversationId || asked >= AGAIN_MS.length) return;
    if (!now.map[id]?.some((page) => page.before && !page.after)) return;
    clearTimeout(timer);
    timer = setTimeout(() => { asked++; void refetch(); }, AGAIN_MS[asked]);
  });
  return () => {
    const now = newest() ? pictured.latest : undefined;
    return now?.conversation === props.conversationId ? now.map : undefined;
  };
}

export function BeforeAfter(props: { conversationId: string; messageId: string }) {
  const pictured = useContext(AnswerPictures);
  const pages = () => pictured()?.[props.messageId];
  // Asking again keeps the rows already drawn when their pages did not change.
  const pairs = createMemo(
    (): Pair[] => (pages() ?? []).flatMap((page) => (page.before && page.after ? [{ page: page.page, before: page.before, after: page.after }] : [])),
    [],
    { equals: (a, b) => a.length === b.length && a.every((pair, i) => pair.before === b[i]!.before && pair.after === b[i]!.after) },
  );
  // A row's pictures load while it is in view (the thread's shared watcher), each once as a data URL; the row leaving the
  // screen, another conversation or a pair going away lets their bytes go, and coming back reads them again.
  const media = useContext(ReplyMediaContext);
  const [seen, setSeen] = createSignal(!media);
  const [row, setRow] = createSignal<HTMLDivElement>();
  createEffect(() => {
    const session = media?.();
    const element = row();
    if (!element || !session) return;
    onCleanup(session.observe(element, setSeen));
  });
  const kept = new Map<string, string | null>();
  // No pair or not in view, no source: the fetcher never runs (an empty string would still run it).
  const names = createMemo(() => seen() && pairs().length > 0 && `${props.conversationId}|${pairs().flatMap((pair) => [pair.before, pair.after]).join(' ')}`);
  const [loaded, { mutate }] = createResource(names, async (list) => {
    const [conversation, all] = list.split('|') as [string, string];
    const wanted = all.split(' ').map((name) => `${conversation}/${name}`);
    for (const key of kept.keys()) if (!wanted.includes(key)) kept.delete(key);
    const got = await Promise.all(wanted.filter((key) => !kept.has(key)).map(async (key) => [key, await api.sitePicture(conversation, key.slice(conversation.length + 1)).catch(() => null)] as const));
    // A read that ends after the row left the screen or changed keeps nothing.
    if (untrack(names) !== list) return undefined;
    for (const [key, src] of got) kept.set(key, src);
    return { conversation, pictures: new Map(wanted.map((key) => [key.slice(conversation.length + 1), kept.get(key) ?? null])) };
  });
  createEffect(on(names, (list) => {
    if (list) return;
    kept.clear();
    mutate(undefined);
  }));
  onCleanup(() => kept.clear());
  const picture = (name: string) => {
    const now = loaded.latest;
    return now?.conversation === props.conversationId ? now.pictures.get(name) ?? undefined : undefined;
  };
  const label = createUniqueId();
  const alt = (pair: Pair, side: Side) => t(side === 'before' ? 'answer.pictures.pageBefore' : 'answer.pictures.pageAfter', { page: pair.page });
  // The viewer moves through this answer's pictures only, in the order the row shows them.
  const open = (img: HTMLImageElement, pair: Pair, side: Side) => {
    const shots = pairs().flatMap((p) => SIDES.flatMap((s) => {
      const src = picture(p[s]);
      return src ? [{ src, alt: alt(p, s), at: p.page === pair.page && s === side }] : [];
    }));
    openViewer(img, shots.map(({ src, alt }) => ({ src, alt })), Math.max(0, shots.findIndex((shot) => shot.at)));
  };
  return (
    <Show when={pairs().length}>
      <div ref={setRow} class="before-after" role="group" aria-labelledby={label}>
        <span class="before-after-head" id={label}>{t('answer.pictures.label')}</span>
        <div class="before-after-pages">
          <For each={pairs()}>
            {(pair) => (
              <div class="before-after-page">
                <For each={SIDES}>
                  {(side) => (
                    <figure class="before-after-shot" data-side={side}>
                      <span class="thumb">
                        <Show when={picture(pair[side])}>
                          {(src) => (
                            <button type="button" class="thumb-open" onClick={(e) => open(e.currentTarget.firstElementChild as HTMLImageElement, pair, side)}>
                              <img data-view src={src()} alt={alt(pair, side)} draggable={false} decoding="async" />
                            </button>
                          )}
                        </Show>
                      </span>
                      <figcaption>{t(side === 'before' ? 'answer.pictures.before' : 'answer.pictures.after')}</figcaption>
                    </figure>
                  )}
                </For>
              </div>
            )}
          </For>
        </div>
      </div>
    </Show>
  );
}
