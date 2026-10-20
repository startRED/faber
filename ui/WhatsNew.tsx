import { For, onCleanup, onMount } from 'solid-js';
import { keepFocusIn } from './focus.ts';
import { t } from './i18n.ts';
import { pressedAny } from './keys.ts';
import type { Key } from './locales/en.ts';
import { Mark } from './Mark.tsx';
import { Icon } from './Glyphs.tsx';
import { grouped, type Note } from './notes.ts';

// A small mark for each kind, from the shell's own icons.
const MARKS = { new: 'plus', better: 'up', fixed: 'check' } as const;

/**
 * "What's new": one calm screen on the first start after an update, drawn in the stage's own surface rather than a card
 * over a dim. Focus starts on Continue, Esc closes it too, and the screen behind is inert. It is a plain page of text, so
 * closing it removes the whole of it.
 */
export function WhatsNew(props: { version: string; notes: Note[]; onClose: () => void }) {
  let button: HTMLButtonElement | undefined;
  let root: HTMLDivElement | undefined;
  // The entries are numbered across the groups, so they arrive one after the other.
  let n = 0;
  const groups = grouped(props.notes).map((g) => ({ ...g, first: n, notes: g.notes.map((note) => ({ note, at: n++ })) }));
  onMount(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    queueMicrotask(() => button?.focus());
    // Capture phase, so the shortcuts of the shell never see these keys.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      } else if (e.key === 'Tab' && !e.ctrlKey && !e.metaKey && !e.altKey) {
        keepFocusIn(e, root);
      } else if (pressedAny(e)) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener('keydown', onKey, true);
    onCleanup(() => {
      window.removeEventListener('keydown', onKey, true);
      if (before?.isConnected) before.focus();
    });
  });
  return (
    <div class="notes" data-surface="whatsnew" ref={(el) => (root = el)} role="dialog" aria-modal="true" aria-labelledby="notes-title">
      <div class="notes-col">
        <Mark size={28} />
        <p class="notes-ver num">{t('notes.version', { version: props.version })}</p>
        <h1 id="notes-title" class="display">
          {t('notes.title')}
        </h1>
        <For each={groups}>
          {(group) => (
            <section class="notes-group" data-kind={group.kind}>
              <h2 style={{ '--i': group.first }}>
                <Icon name={MARKS[group.kind]} small />
                {t(`notes.kind.${group.kind}` as Key)}
              </h2>
              <ul>
                <For each={group.notes}>{({ note, at }) => <li style={{ '--i': at + 1 }}>{t(note.key)}</li>}</For>
              </ul>
            </section>
          )}
        </For>
        <div class="notes-act">
          <button type="button" class="btn primary big" ref={(el) => (button = el)} onClick={() => props.onClose()}>
            {t('notes.continue')}
          </button>
        </div>
      </div>
    </div>
  );
}
