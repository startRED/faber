import { createMemo, createSignal, For, Show } from 'solid-js';
import type { Image } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { openViewer } from './Viewer.tsx';

/** What a file card says it is: the extension in capitals ("PDF"), or the plain word when the name has none. */
const kindOf = (name: string) => /\.([A-Za-z0-9]{1,5})$/.exec(name)?.[1]?.toUpperCase() ?? t('thumb.file');

/** One attachment, as the message bubble and the composer both draw it: a small rounded picture that opens full screen, or a
 * card with the file's icon, name and kind. With `onRemove` it has its own × at the top right, shown on hover and focus. */
export function Thumb(props: { name: string; src?: string; onRemove?: () => void }) {
  return (
    <div class="thumb" classList={{ file: !props.src }}>
      <Show
        when={props.src}
        fallback={
          <span class="thumb-card" title={props.name}>
            <span class="fi">
              <Icon name="sheet" small />
            </span>
            <span class="thumb-text">
              <b>{props.name}</b>
              <small>{kindOf(props.name)}</small>
            </span>
          </span>
        }
      >
        {(src) => (
          <button type="button" class="thumb-open" aria-label={t('viewer.open', { name: props.name })} title={props.name} onClick={(e) => openViewer(e.currentTarget.firstElementChild as HTMLImageElement)}>
            <img data-view src={src()} alt={props.name} draggable={false} decoding="async" />
          </button>
        )}
      </Show>
      <Show when={props.onRemove}>
        {(remove) => (
          <button type="button" class="thumb-x" aria-label={t('composer.removeOne', { name: props.name })} title={t('composer.removeOne', { name: props.name })} onClick={() => remove()()}>
            <Icon name="close" small />
          </button>
        )}
      </Show>
    </div>
  );
}

const SHOWN = 5;

/** The pictures and files of a message above its text: one wrapped row of small thumbnails, and "+N" for the rest, which opens
 * them. The files are only names (the engine keeps nothing else of them). */
export function Strip(props: { images?: Image[]; files?: string[] }) {
  const [all, setAll] = createSignal(false);
  const pieces = createMemo(() => [
    ...(props.images ?? []).map((img) => ({ name: t('mine.image'), src: `data:${img.mediaType};base64,${img.base64}` })),
    ...(props.files ?? []).map((name) => ({ name, src: undefined as string | undefined })),
  ]);
  const folded = () => !all() && pieces().length > SHOWN;
  const shown = () => (folded() ? pieces().slice(0, SHOWN - 1) : pieces());
  return (
    <Show when={pieces().length}>
      <div class="thumbs">
        <For each={shown()}>{(piece, i) => <div class="thumb-slot" classList={{ late: all() && i() >= SHOWN - 1 }} style={{ '--i': i() - (SHOWN - 1) }}><Thumb name={piece.name} src={piece.src} /></div>}</For>
        <Show when={folded()}>
          <button type="button" class="thumb thumb-more" aria-label={t('thumb.more', { n: pieces().length - (SHOWN - 1) })} onClick={() => setAll(true)}>
            +{pieces().length - (SHOWN - 1)}
          </button>
        </Show>
      </div>
    </Show>
  );
}
