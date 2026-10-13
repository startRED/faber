import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import type { Experience } from './api.ts';
import { keepFocusIn } from './focus.ts';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';
import { Mark } from './Mark.tsx';
import { animateList } from './motion.ts';
import { type Box, type Placement, placeCard, type TourStep, tourFor } from './tour.ts';

/** Where the element a card points at is on screen now, or none when it is missing or hidden (the Projects button below
 * 900 px, where the sidebar is behind its button). */
function anchorBox(anchor: string): Box | undefined {
  const el = document.querySelector<HTMLElement>(`[data-tour="${anchor}"]`);
  if (!el || el.getClientRects().length === 0 || el.closest('[aria-hidden="true"]') || getComputedStyle(el).visibility === 'hidden') return undefined;
  const r = el.getBoundingClientRect();
  return r.width > 0 && r.height > 0 ? { x: r.x, y: r.y, w: r.width, h: r.height } : undefined;
}

const RING = 6;

/** One card: Curioso, the step, one or two sentences, and Next and Skip. It is a modal dialog: the screen behind it is
 * inert, focus starts on Next and stays on the card, and Esc skips the whole tutorial. */
function Card(props: { step: TourStep; n: number; total: number; onNext: () => void; onSkip: () => void; onHole: (box: Box | undefined) => void }) {
  const [at, setAt] = createSignal<Placement>();
  let card: HTMLDivElement | undefined;
  let next: HTMLButtonElement | undefined;
  const last = () => props.n === props.total;
  const measure = () => {
    const box = anchorBox(props.step.anchor);
    props.onHole(box);
    const size = card!.getBoundingClientRect();
    const headings = [...document.querySelectorAll<HTMLElement>('h1')]
      .map((h) => h.getBoundingClientRect())
      .filter((r) => r.width > 0)
      .map((r): Box => ({ x: r.x, y: r.y, w: r.width, h: r.height }));
    const view = { w: window.innerWidth, h: window.innerHeight };
    setAt(placeCard(box, { w: size.width, h: size.height }, view, props.step.sides, headings));
  };
  onMount(() => {
    measure();
    // The card is drawn once it has a place, which is after this batch; a hidden button cannot take focus.
    queueMicrotask(() => next?.focus());
    window.addEventListener('resize', measure);
  });
  onCleanup(() => window.removeEventListener('resize', measure));
  const key = (part: 'title' | 'body') => t(`tour.${props.step.id}.${part}` as Key);
  return (
    <div
      class="coach"
      data-surface="tutorial.card"
      ref={(el) => (card = el)}
      role="dialog"
      aria-modal="true"
      aria-labelledby="coach-title"
      aria-describedby="coach-body"
      data-side={at()?.side}
      style={{
        left: `${at()?.x ?? 0}px`,
        top: `${at()?.y ?? 0}px`,
        '--pointer': `${at()?.pointer ?? 0}px`,
        visibility: at() ? 'visible' : 'hidden',
      }}
    >
      <div class="coach-head">
        <Mark size={22} />
        <span class="coach-n num">{t('tour.step', { n: props.n, total: props.total })}</span>
      </div>
      <h2 id="coach-title" class="display">
        {key('title')}
      </h2>
      <p id="coach-body">{key('body')}</p>
      <div class="coach-act">
        <button type="button" class="btn primary" ref={(el) => (next = el)} onClick={() => props.onNext()}>
          {last() ? t('tour.finish') : t('tour.next')}
        </button>
        <Show when={!last()}>
          <button type="button" class="btn ghost" onClick={() => props.onSkip()}>
            {t('tour.skip')}
          </button>
        </Show>
      </div>
    </div>
  );
}

/**
 * The tutorial: coach cards that point at the real screen. A beginner gets three, an experienced person two. Finishing
 * or skipping calls `onEnd` once; the screen saves `tutorialDone` and gives focus back.
 */
export function Tutorial(props: { experience?: Experience; onEnd: () => void }) {
  // The cards are chosen once, when the tutorial starts.
  const steps = tourFor(props.experience);
  const [index, setIndex] = createSignal(0);
  const [hole, setHole] = createSignal<Box>();
  let ended = false;
  const end = () => {
    if (ended) return;
    ended = true;
    props.onEnd();
  };
  const next = () => (index() + 1 < steps.length ? setIndex(index() + 1) : end());

  onMount(() => {
    // Capture phase, so the screen's own shortcuts never see these keys: Esc skips, Tab stays on the card, and the
    // shortcuts of the shell (Ctrl N, Ctrl ,, Ctrl .) do nothing.
    const onKey = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        end();
      } else if (e.key === 'Tab' && !ctrl && !e.altKey) {
        keepFocusIn(e, document.querySelector('.coach'));
      } else if (ctrl && ['n', ',', '.'].includes(e.key.toLowerCase())) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener('keydown', onKey, true);
    onCleanup(() => window.removeEventListener('keydown', onKey, true));
  });

  return (
    <div class="tour" ref={animateList}>
      <div class="tour-dim" classList={{ clear: !!hole() }} />
      <Show when={hole()}>
        {(box) => (
          <div
            class="tour-ring"
            style={{ transform: `translate(${box().x - RING}px, ${box().y - RING}px)`, width: `${box().w + RING * 2}px`, height: `${box().h + RING * 2}px` }}
          />
        )}
      </Show>
      {/* A new card for each step, so a screen reader announces it and focus starts on its Next button. */}
      <Show when={index() + 1} keyed>
        {(n) => <Card step={steps[n - 1]!} n={n} total={steps.length} onNext={next} onSkip={end} onHole={setHole} />}
      </Show>
    </div>
  );
}
