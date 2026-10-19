import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { keepFocusIn } from './focus.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

interface Shot {
  src: string;
  alt: string;
}

const [viewer, setViewer] = createSignal<{ shots: Shot[]; at: number } | undefined>();

/**
 * A thumbnail the person can open: one button around the picture, so the keyboard reaches it too. The pictures that share the
 * nearest `[data-gallery]` ancestor (a message box, a whole conversation) are the ones the viewer steps through.
 */
export function Thumb(props: { src: string; alt: string }) {
  return (
    <button
      type="button"
      class="zoom"
      aria-label={`${t('image.open')}: ${props.alt}`}
      onClick={(e) => {
        const img = e.currentTarget.querySelector('img')!;
        const shots = [...(img.closest('[data-gallery]') ?? document).querySelectorAll<HTMLImageElement>('img[data-zoom]')];
        setViewer({ shots: shots.map((s) => ({ src: s.src, alt: s.alt })), at: Math.max(0, shots.indexOf(img)) });
      }}
    >
      <img data-zoom src={props.src} alt={props.alt} draggable={false} />
    </button>
  );
}

/** The picture opened full size over the screen: arrows on the sides and the keyboard step through the others, Esc closes. */
export function Lightbox() {
  return (
    <Show when={viewer()}>
      <Open />
    </Show>
  );
}

function Open() {
  let root: HTMLDivElement | undefined;
  let close: HTMLButtonElement | undefined;
  const view = () => viewer()!;
  const total = () => view().shots.length;
  const go = (step: number) => setViewer((v) => v && { ...v, at: Math.min(v.shots.length - 1, Math.max(0, v.at + step)) });
  onMount(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    queueMicrotask(() => close?.focus());
    // Capture phase, so the shortcuts of the shell never see these keys.
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === 'Tab') return keepFocusIn(e, root);
      if (e.key === 'Escape') setViewer(undefined);
      else if (e.key === 'ArrowLeft') go(-1);
      else if (e.key === 'ArrowRight') go(1);
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener('keydown', onKey, true);
    onCleanup(() => {
      window.removeEventListener('keydown', onKey, true);
      if (before?.isConnected) before.focus();
    });
  });
  return (
    <div class="lightbox" ref={(el) => (root = el)} role="dialog" aria-modal="true" aria-label={t('image.viewer')} onClick={(e) => e.target === e.currentTarget && setViewer(undefined)}>
      <button type="button" class="lb-btn lb-close" ref={(el) => (close = el)} aria-label={t('image.close')} title={t('image.close')} onClick={() => setViewer(undefined)}>
        <Icon name="close" />
      </button>
      <Show when={total() > 1}>
        <button type="button" class="lb-btn lb-prev" disabled={view().at === 0} aria-label={t('image.previous')} title={t('image.previous')} onClick={() => go(-1)}>
          <Icon name="left" />
        </button>
        <button type="button" class="lb-btn lb-next" disabled={view().at === total() - 1} aria-label={t('image.next')} title={t('image.next')} onClick={() => go(1)}>
          <Icon name="right" />
        </button>
        <span class="lb-count num" aria-live="polite">
          {t('image.count', { n: view().at + 1, total: total() })}
        </span>
      </Show>
      <img src={view().shots[view().at].src} alt={view().shots[view().at].alt} draggable={false} />
    </div>
  );
}
