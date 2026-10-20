import { createEffect, createSignal, on, Show } from 'solid-js';
import { keepFocusIn } from './focus.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

export interface Picture {
  mediaType: string;
  base64: string;
}

const [viewing, setViewing] = createSignal<Picture>();

const source = (img: Picture) => `data:${img.mediaType};base64,${img.base64}`;

/** An image the person attached, small, that opens large on a click (the composer's and the conversation's). */
export function Pic(props: { img: Picture }) {
  return (
    <button type="button" class="pic" aria-label={t('image.open')} title={t('image.open')} onClick={() => setViewing(props.img)}>
      <img alt={t('mine.image')} src={source(props.img)} />
    </button>
  );
}

/** The large view: one dialog for the whole screen, built only while an image is open, with no motion. Escape, the button
 * or a click beside the image close it, and focus goes back to the image that opened it. */
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
  return (
    <Show when={viewing()}>
      {(img) => (
        <div
          class="viewer-scrim"
          onPointerDown={(e) => e.target === e.currentTarget && close()}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.stopPropagation();
              close();
            } else if (e.key === 'Tab') keepFocusIn(e, dialog);
          }}
        >
          <div class="viewer" role="dialog" aria-modal="true" aria-label={t('mine.image')} ref={(el) => (dialog = el)}>
            <img alt={t('mine.image')} src={source(img())} />
            <button type="button" class="iconbtn viewer-close" aria-label={t('window.close')} title={t('window.close')} onClick={close} ref={(el) => queueMicrotask(() => el.focus())}>
              <Icon name="close" />
            </button>
          </div>
        </div>
      )}
    </Show>
  );
}
