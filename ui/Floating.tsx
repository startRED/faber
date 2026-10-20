import { createEffect, createSignal, onCleanup, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { keepForExit, level } from './motion.ts';

// Two small things that float over the conversation and exist only while they are of use.

interface Spot {
  x: number;
  y: number;
  below: boolean;
  text: string;
}

/** The selected words as a quote for the message box: "> " before every line, and a blank line after, ready to be written under. */
export const quoted = (text: string) => `${text.trim().split(/\r?\n/).map((line) => `> ${line}`.trimEnd()).join('\n')}\n\n`;

/** Puts a quote at the end of the message box and focuses it. insertText keeps the field's undo history; the fallback writes the text itself and tells the page. */
export function quoteInBox(text: string): void {
  const box = document.querySelector<HTMLTextAreaElement>('.composer textarea');
  if (!box) return;
  box.focus();
  box.setSelectionRange(box.value.length, box.value.length);
  const quote = (box.value && !box.value.endsWith('\n\n') ? (box.value.endsWith('\n') ? '\n' : '\n\n') : '') + quoted(text);
  if (document.execCommand('insertText', false, quote)) return;
  box.value += quote;
  box.dispatchEvent(new Event('input', { bubbles: true }));
}

/**
 * "Ask about this": while some words of one answer are selected, a small button sits over them. It puts them in the message box as a
 * quote. One set of listeners serves every answer, and they exist only while a thread does.
 */
export function Quote() {
  const [spot, setSpot] = createSignal<Spot>();
  const [held, setHeld] = createSignal<Spot>();
  const shown = keepForExit(() => !!spot());
  createEffect(() => {
    const now = spot();
    if (now) setHeld(now);
  });
  const update = () => {
    const selection = getSelection();
    const text = selection?.toString().trim() ?? '';
    const node = selection?.rangeCount ? selection.getRangeAt(0).commonAncestorContainer : undefined;
    const inside = node instanceof Element ? node : node?.parentElement;
    if (!selection || !text || !inside?.closest('.ai') || inside.closest('summary')) return setSpot(undefined);
    const box = selection.getRangeAt(0).getBoundingClientRect();
    // Above the words, or under them when they are at the top edge of the window.
    const below = box.top < 56;
    setSpot({ x: Math.min(innerWidth - 80, Math.max(80, box.left + box.width / 2)), y: below ? box.bottom + 8 : box.top - 8, below, text });
  };
  onMount(() => {
    const hide = () => {
      if (!getSelection()?.toString().trim()) setSpot(undefined);
    };
    // A drag or Shift+arrows is finished at pointerup and keyup; a click that clears the selection is seen at once.
    document.addEventListener('selectionchange', hide);
    document.addEventListener('pointerup', update);
    document.addEventListener('keyup', update);
    onCleanup(() => {
      document.removeEventListener('selectionchange', hide);
      document.removeEventListener('pointerup', update);
      document.removeEventListener('keyup', update);
    });
  });
  return (
    <Show when={shown() ? held() : undefined} keyed>
      {(s) => (
        <Portal>
          <div class="quote-pos" classList={{ below: s.below }} style={{ left: `${s.x}px`, top: `${s.y}px` }}>
            <button
              type="button"
              class="quote"
              classList={{ leaving: !spot() }}
              // Pressing must not clear the selection it is about to quote.
              onPointerDown={(e) => e.preventDefault()}
              onClick={() => {
                quoteInBox(s.text);
                getSelection()?.removeAllRanges();
                setSpot(undefined);
              }}
            >
              {t('quote.ask')}
            </button>
          </div>
        </Portal>
      )}
    </Show>
  );
}

/** How far from the end the person has to be before the way back shows. */
const AWAY = 240;

/**
 * "Latest": a button over the end of the thread while the person has scrolled up, most of all while an answer is streaming below.
 * It sits in the thread's own box, so the motion list around it gives it its entrance and exit.
 */
export function ToEnd(props: { thread: () => HTMLElement | undefined }) {
  const [away, setAway] = createSignal(false);
  onMount(() => {
    const el = props.thread();
    if (!el) return;
    let watched: Element | null = null;
    // The column grows as an answer streams without any scroll event, so its size is watched too (once per frame at most).
    const sizes = new ResizeObserver(() => check());
    const check = () => {
      setAway(el.scrollHeight - el.scrollTop - el.clientHeight > AWAY);
      if (el.firstElementChild !== watched) {
        if (watched) sizes.unobserve(watched);
        watched = el.firstElementChild;
        if (watched) sizes.observe(watched);
      }
    };
    el.addEventListener('scroll', check, { passive: true });
    check();
    onCleanup(() => {
      el.removeEventListener('scroll', check);
      sizes.disconnect();
    });
  });
  const go = () => {
    const el = props.thread();
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: level(el) === 'full' ? 'smooth' : 'auto' });
    // An answer still streaming grows while the scroll runs: when it stops short, the rest is one jump. Only the scroll this press
    // started: a person who scrolls up a second later is never pulled back.
    const pressed = performance.now();
    el.addEventListener(
      'scrollend',
      () => {
        const left = el.scrollHeight - el.scrollTop - el.clientHeight;
        if (performance.now() - pressed < 1500 && left > 4 && left < AWAY) el.scrollTop = el.scrollHeight;
      },
      { once: true },
    );
  };
  return (
    <Show when={away()}>
      <button type="button" class="toend" aria-label={t('thread.toEndLabel')} onClick={go}>
        <Icon name="end" small />
        <span>{t('thread.toEnd')}</span>
      </button>
    </Show>
  );
}
