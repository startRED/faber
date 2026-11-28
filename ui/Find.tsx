import { createEffect, createSignal, on, onCleanup, onMount, Show } from 'solid-js';
import { bringIntoView, LIMIT, paint, rangesIn, unpaint } from './find.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { pressed } from './keys.ts';
import { keepForExit } from './motion.ts';

/** How long the thread may keep changing (an answer streaming in) before the matches are looked for again. */
const SETTLE = 250;

const none = { ranges: [] as Range[], capped: false };
// The thread's own motion level, as every other motion reads it (`motion.ts`): the nearest `data-motion` above it.
const smooth = (root: HTMLElement) =>
  root.closest<HTMLElement>('[data-motion]')?.dataset.motion === 'full' && !matchMedia('(prefers-reduced-motion: reduce)').matches;
// Another layer on top of the shell owns the keys until it closes.
const covered = () => !!document.querySelector('.pal-scrim:not(.leaving), .cmenu:not(.leaving), .ctx-panel:not(.leaving)');

/** Ctrl+F in an open conversation: a small bar over the top of the thread that counts and paints every place a word appears and
 * goes from one to the next. Its elements, its observer and the paint exist only while it is open. */
export function Find(props: { enabled: boolean; root: () => HTMLElement | undefined }) {
  const [open, setOpen] = createSignal(false);
  const shown = keepForExit(open);
  const [query, setQuery] = createSignal('');
  const [found, setFound] = createSignal(none);
  const [at, setAt] = createSignal(0);
  const [exitCount, setExitCount] = createSignal('');
  let input: HTMLInputElement | undefined;
  let opener: HTMLElement | undefined;

  const total = () => found().ranges.length;
  const count = () => {
    if (!open()) return exitCount();
    return !query().trim() ? '' : total() ? t('find.count', { current: at() + 1, total: found().capped ? `${LIMIT}+` : total() }) : t('find.none');
  };
  // Looks for the word again. A new word starts at its first match and goes there; the same word again (the thread changed)
  // stays on the match the person was at.
  const look = (restart: boolean) => {
    const root = props.root();
    const next = root && open() ? rangesIn(root, query()) : none;
    setFound(next);
    setAt((i) => (restart ? 0 : Math.min(i, Math.max(0, next.ranges.length - 1))));
    const first = next.ranges[at()];
    if (restart && first && root) bringIntoView(root, first, smooth(root));
  };
  const go = (step: number) => {
    const n = total();
    const root = props.root();
    if (!n || !root) return;
    setAt((i) => (i + step + n) % n);
    bringIntoView(root, found().ranges[at()]!, smooth(root));
  };
  createEffect(() => paint(found().ranges, found().ranges[at()]));
  const hide = (back: boolean) => {
    if (open()) setExitCount(count());
    setOpen(false);
    // The matches hold on to the text they point into.
    setFound(none);
    if (back && opener?.isConnected) opener.focus({ preventScroll: true });
  };
  createEffect(() => {
    if (!props.enabled) hide(false);
  });
  // The thread changes while an answer is written: look again once it has been quiet for a moment.
  createEffect(() => {
    const root = open() ? props.root() : undefined;
    if (!root) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const watch = new MutationObserver(() => {
      timer ??= setTimeout(() => ((timer = undefined), look(false)), SETTLE);
    });
    watch.observe(root, { childList: true, subtree: true, characterData: true });
    onCleanup(() => (watch.disconnect(), clearTimeout(timer)));
  });
  createEffect(on(query, () => look(true), { defer: true }));

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (pressed(e, 'find')) {
        // The browser's own find never shows: Faber's is the only one.
        e.preventDefault();
        if (!props.enabled || covered()) return;
        if (!open()) {
          opener = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
          // A word selected in the thread is what the person means to look for.
          const picked = getSelection();
          const word = picked?.toString().trim() ?? '';
          if (word && word.length <= 80 && !word.includes('\n') && props.root()?.contains(picked?.anchorNode ?? null)) setQuery(word);
          setOpen(true);
          look(false);
        }
        queueMicrotask(() => (input?.focus(), input?.select()));
      } else if (e.key === 'Escape' && open() && !covered()) {
        // The top layer answers Esc: the bar closes before the sidebar or the running column do.
        e.preventDefault();
        e.stopPropagation();
        hide(true);
      }
    };
    window.addEventListener('keydown', onKey, true);
    onCleanup(() => {
      window.removeEventListener('keydown', onKey, true);
      unpaint();
    });
  });

  return (
    <Show when={shown()}>
      <div class="find" classList={{ leaving: !open() }} role="search" aria-label={t('find.label')} inert={!open()}>
        <label class="field search">
          <Icon name="search" small />
          <input
            type="search"
            ref={(el) => (input = el)}
            value={query()}
            placeholder={t('find.placeholder')}
            aria-label={t('find.label')}
            autocomplete="off"
            spellcheck={false}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key !== 'Enter' || e.isComposing) return;
              e.preventDefault();
              go(e.shiftKey ? -1 : 1);
            }}
          />
        </label>
        <span class="find-count num" role="status">{count()}</span>
        <button type="button" class="iconbtn" title={t('find.prev')} aria-label={t('find.prev')} disabled={!total()} onClick={() => go(-1)}>
          <Icon name="up" small />
        </button>
        <button type="button" class="iconbtn" title={t('find.next')} aria-label={t('find.next')} disabled={!total()} onClick={() => go(1)}>
          <Icon name="down" small />
        </button>
        <button type="button" class="iconbtn" title={t('find.close')} aria-label={t('find.close')} onClick={() => hide(true)}>
          <Icon name="close" small />
        </button>
      </div>
    </Show>
  );
}
