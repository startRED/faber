import { createEffect, createSignal, onCleanup, onMount, Show } from 'solid-js';
import { render } from 'solid-js/web';
import { api } from './api.ts';
import { keepFocusIn } from './focus.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { level, millis } from './motion.ts';

// The full-screen picture viewer. Its DOM exists only while it is open: opening mounts it on the body, closing takes it away.
// It holds no state of its own between uses, and the pictures it moves between are read from the page when it opens.
type Shot = {
  src: string;
  alt: string;
  href?: string;
} | {
  alt: string;
  href?: string;
  load: () => Promise<{ src: string; release: () => void }>;
};

/** Every picture a person can open: thumbnails, and the pictures inside an answer. */
const VIEWABLE = 'img[data-view], .md img';
let opened = false;

/** Opens `img` full screen. The pictures of its conversation (or of the composer it sits in) are the ones the arrows move between. */
export function openViewer(img: HTMLElement, gallery?: Shot[], index?: number, release?: () => void): (() => void) | undefined {
  if (opened) { release?.(); return; }
  opened = true;
  const scope = img.closest('.composer') ?? img.closest('.col') ?? document.body;
  const seen = [...scope.querySelectorAll<HTMLImageElement>(VIEWABLE)].filter((el) => el === img || el.getClientRects().length > 0);
  const shots = gallery ?? seen.map((el) => ({ src: el.currentSrc || el.src, alt: el.alt }));
  const host = document.createElement('div');
  host.dataset.motion = level(img);
  document.body.append(host);
  const back = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
  let dispose: () => void;
  const close = () => {
    if (!host.isConnected) return;
    dispose();
    host.remove();
    opened = false;
    release?.();
    if (back?.isConnected) back.focus({ preventScroll: true });
  };
  dispose = render(
    () => (
      <Viewer
        shots={shots}
        index={index ?? Math.max(0, seen.findIndex((image) => image === img))}
        origin={img}
        full={level(img) === 'full'}
        done={close}
      />
    ),
    host,
  );
  return close;
}

/** Where the picture starts when it grows out of its thumbnail (and ends when it goes back): the thumbnail's box, cropped as the
 * thumbnail crops it. */
function fromThumb(small: DOMRect, big: DOMRect): Keyframe {
  const scale = Math.max(small.width / big.width, small.height / big.height);
  const dx = small.left + small.width / 2 - (big.left + big.width / 2);
  const dy = small.top + small.height / 2 - (big.top + big.height / 2);
  const cx = Math.max(0, (big.width * scale - small.width) / 2 / scale);
  const cy = Math.max(0, (big.height * scale - small.height) / 2 / scale);
  return { transform: `translate(${dx}px, ${dy}px) scale(${scale})`, clipPath: `inset(${cy}px ${cx}px round ${8 / scale}px)` };
}
const RESTING: Keyframe = { transform: 'translate(0, 0) scale(1)', clipPath: 'inset(0px 0px round 0px)' };

function Viewer(props: { shots: Shot[]; index: number; origin: HTMLElement; full: boolean; done: () => void }) {
  const [at, setAt] = createSignal(props.index);
  const [leaving, setLeaving] = createSignal(false);
  let dialog!: HTMLDivElement;
  let picture: HTMLImageElement | undefined;
  let first = true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  const shot = () => props.shots[at()]!;
  const [loaded, setLoaded] = createSignal<{ shot: Shot; src: string | undefined }>();
  createEffect(() => {
    const current = shot();
    if (!('load' in current)) return;
    let active = true;
    let release: (() => void) | undefined;
    onCleanup(() => {
      active = false;
      release?.();
      setLoaded(undefined);
    });
    void current.load().then((image) => {
      if (!active) return image.release();
      release = image.release;
      setLoaded({ shot: current, src: image.src });
    }, () => { if (active) setLoaded({ shot: current, src: undefined }); });
  });
  const source = () => {
    const current = shot();
    if ('src' in current) return current.src;
    const image = loaded();
    return image?.shot === current ? image.src : undefined;
  };
  const last = () => props.shots.length - 1;
  const move = (by: number) => {
    setAt((i) => Math.min(last(), Math.max(0, i + by)));
    // The arrow that was pressed goes away at either end, and with it the focus: it stays in the dialog, which still takes Esc and the arrows.
    if (!dialog.contains(document.activeElement)) dialog.focus({ preventScroll: true });
  };
  const options = () => ({
    duration: millis(dialog, '--dur-ui', 220),
    easing: getComputedStyle(dialog).getPropertyValue('--ease-spring').trim() || 'ease-out',
  });
  // The picture grows out of the thumbnail it was opened from, and only when that is what is still on screen.
  const grow = (el: HTMLImageElement) => {
    el.style.opacity = '';
    if (props.full && props.origin.isConnected) el.animate([fromThumb(props.origin.getBoundingClientRect(), el.getBoundingClientRect()), RESTING], { ...options(), fill: 'backwards' });
  };
  const close = () => {
    if (leaving()) return;
    setLeaving(true);
    if (!props.full) return props.done();
    const small = props.origin.isConnected && at() === props.index ? props.origin.getBoundingClientRect() : undefined;
    const onScreen = small && small.bottom > 0 && small.top < innerHeight && small.right > 0 && small.left < innerWidth;
    if (onScreen && picture) picture.animate([RESTING, fromThumb(small, picture.getBoundingClientRect())], { ...options(), duration: millis(dialog, '--dur-exit', 180), easing: 'ease-in', fill: 'forwards' });
    timer = setTimeout(props.done, millis(dialog, '--dur-exit', 180));
  };
  onMount(() => dialog.focus({ preventScroll: true }));
  return (
    <div
      ref={(el) => (dialog = el)}
      class="viewer"
      classList={{ leaving: leaving() }}
      role="dialog"
      aria-modal="true"
      aria-label={t('viewer.label')}
      tabindex="-1"
      inert={leaving()}
      onKeyDown={(e) => {
        if (e.key === 'Escape') close();
        else if (e.key === 'ArrowLeft') move(-1);
        else if (e.key === 'ArrowRight') move(1);
        else if (e.key === 'Tab') {
          // The screen's own focus trap (a side panel, a first-run step) must not pull the focus out from under the viewer.
          e.stopPropagation();
          return keepFocusIn(e, dialog);
        } else return;
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <div class="viewer-stage" onClick={(e) => e.target === e.currentTarget && close()}>
        <Show when={source()} keyed fallback={<span role="status">
          {t(loaded()?.shot === shot() ? 'media.gone' : 'file.loading')}
          <Show when={loaded()?.shot === shot() && shot().href}>{href =>
            <> · <a class="link" href={href()} onClick={event => { event.preventDefault(); void api.openLink(href()); }}>{shot().alt}</a></>
          }</Show>
        </span>}>
          {(src) => {
            const swapping = !first;
            first = false;
            return (
              <img
                ref={(el) => {
                  picture = el;
                  if (!swapping) {
                    el.style.opacity = '0';
                    void el.decode().then(() => grow(el), () => grow(el));
                  }
                }}
                class="viewer-img"
                classList={{ fresh: swapping }}
                src={src}
                alt={shot().alt}
                onError={() => {
                  const current = shot();
                  if (current.href && source() === src) setLoaded({ shot: current, src: undefined });
                }}
              />
            );
          }}
        </Show>
      </div>
      <button type="button" class="viewer-btn close" aria-label={t('viewer.close')} title={t('viewer.close')} onClick={close}>
        <Icon name="close" />
      </button>
      <Show when={props.shots.length > 1}>
        <Show when={at() > 0}>
          <button type="button" class="viewer-btn prev" aria-label={t('viewer.prev')} title={t('viewer.prev')} onClick={() => move(-1)}>
            <Icon name="left" />
          </button>
        </Show>
        <Show when={at() < last()}>
          <button type="button" class="viewer-btn next" aria-label={t('viewer.next')} title={t('viewer.next')} onClick={() => move(1)}>
            <Icon name="right" />
          </button>
        </Show>
        <span class="viewer-count" aria-live="polite">{t('viewer.count', { n: at() + 1, total: props.shots.length })}</span>
      </Show>
    </div>
  );
}
