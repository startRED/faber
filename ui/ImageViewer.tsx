import { createEffect, createSignal, on, onCleanup, Show } from 'solid-js';
import { keepFocusIn } from './focus.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

export interface Picture {
  mediaType: string;
  base64: string;
}

interface Shot {
  src: string;
  alt: string;
}

const [viewing, setViewing] = createSignal<{ shots: Shot[]; at: number }>();

const source = (img: Picture) => `data:${img.mediaType};base64,${img.base64}`;

/**
 * A picture that opens large on a click: one button around it, so the keyboard reaches it too. The pictures that share the
 * nearest `[data-gallery]` ancestor (a whole conversation) are the ones the viewer steps through; with none, it shows just
 * this one (the message box's).
 */
export function Thumb(props: { src: string; alt: string }) {
  return (
    <button
      type="button"
      class="pic"
      aria-label={t('image.open')}
      title={t('image.open')}
      onClick={(e) => {
        const img = e.currentTarget.querySelector('img')!;
        const all = [...(img.closest('[data-gallery]')?.querySelectorAll<HTMLImageElement>('img[data-zoom]') ?? [img])];
        setViewing({ shots: all.map((s) => ({ src: s.src, alt: s.alt })), at: Math.max(0, all.indexOf(img)) });
      }}
    >
      <img data-zoom alt={props.alt} src={props.src} draggable={false} />
    </button>
  );
}

/** An image the person attached, small, that opens large on a click (the composer's and the conversation's). */
export function Pic(props: { img: Picture }) {
  return <Thumb alt={t('mine.image')} src={source(props.img)} />;
}

/**
 * The large view: one dialog for the whole screen, built only while a picture is open, with no motion. Arrows on the sides and
 * the left and right keys step through the other pictures of the conversation; Escape, the button or a click beside the
 * picture close it, and focus goes back to the picture that opened it.
 */
export function ImageViewerHost() {
  let dialog: HTMLDivElement | undefined;
  let opener: HTMLElement | undefined;
  createEffect(
    on(
      viewing,
      (now, before) => {
        if (now && !before) opener = document.activeElement as HTMLElement | undefined;
        if (!now) opener?.focus();
      },
      { defer: true },
    ),
  );
  const close = () => setViewing(undefined);
  const total = () => viewing()?.shots.length ?? 0;
  const go = (step: number) => setViewing((v) => v && { ...v, at: Math.min(v.shots.length - 1, Math.max(0, v.at + step)) });
  // Capture phase, so the shortcuts of the shell never see these keys while a picture is open.
  const onKey = (e: KeyboardEvent) => {
    if (!viewing() || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === 'Tab') return keepFocusIn(e, dialog);
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowLeft') go(-1);
    else if (e.key === 'ArrowRight') go(1);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  window.addEventListener('keydown', onKey, true);
  onCleanup(() => window.removeEventListener('keydown', onKey, true));
  return (
    <Show when={viewing()}>
      {(view) => (
        <div class="viewer-scrim" onPointerDown={(e) => e.target === e.currentTarget && close()}>
          <div class="viewer" role="dialog" aria-modal="true" aria-label={t('image.viewer')} ref={(el) => (dialog = el)}>
            <img alt={view().shots[view().at].alt} src={view().shots[view().at].src} draggable={false} />
            <button type="button" class="iconbtn viewer-close" aria-label={t('image.close')} title={t('image.close')} onClick={close} ref={(el) => queueMicrotask(() => el.focus())}>
              <Icon name="close" />
            </button>
          </div>
          <Show when={total() > 1}>
            <button type="button" class="viewer-nav viewer-prev" disabled={view().at === 0} aria-label={t('image.previous')} title={t('image.previous')} onClick={() => go(-1)}>
              <Icon name="left" />
            </button>
            <button type="button" class="viewer-nav viewer-next" disabled={view().at === total() - 1} aria-label={t('image.next')} title={t('image.next')} onClick={() => go(1)}>
              <Icon name="right" />
            </button>
            <span class="viewer-count num" aria-live="polite">
              {t('image.count', { n: view().at + 1, total: total() })}
            </span>
          </Show>
        </div>
      )}
    </Show>
  );
}
