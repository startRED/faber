import { createEffect, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { keepFocusIn } from './focus.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { cap, type Shortcut, SHORTCUTS } from './keys.ts';
import { keepForExit } from './motion.ts';

const [open, setOpen] = createSignal(false);
/** Where focus was before the list opened: it goes back there when the list closes. */
let opener: HTMLElement | undefined;

/** Opens the list of shortcuts ("?", Settings > General and the Ctrl+K box). */
export const openShortcuts = () => {
  opener = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
  setOpen(true);
};
/** Whether the list is open, so the Ctrl+K box waits until it closes. */
export const shortcutsOpen = open;

const TYPING = 'textarea, input, select, [contenteditable]';
const GROUPS = [
  ['anywhere', 'keys.anywhere'],
  ['message', 'keys.message'],
  ['browser', 'keys.browser'],
] as const;

/** "?" outside a text box: every shortcut Faber has, read from ui/keys.ts, in the Ctrl+K box's own surface. Esc, the close
 * button or a click beside it close it, and focus goes back where it was. Its elements exist only while it is open. */
export function Shortcuts(props: { enabled: boolean }) {
  const shown = keepForExit(open);
  let dialog: HTMLElement | undefined;
  const hide = (back: boolean) => {
    setOpen(false);
    if (back && opener?.isConnected) opener.focus({ preventScroll: true });
  };
  createEffect(() => {
    if (!props.enabled) hide(false);
  });

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!open()) {
        const typing = e.target instanceof Element && e.target.closest(TYPING);
        // A "?" typed with AltGr (Ctrl and Alt on Windows) is still the key, not a Ctrl shortcut.
        const plain = !e.metaKey && ((!e.ctrlKey && !e.altKey) || e.getModifierState('AltGraph'));
        if (e.key === '?' && plain && !typing && props.enabled && !document.querySelector('.pal-scrim:not(.leaving), .cmenu:not(.leaving)')) {
          e.preventDefault();
          openShortcuts();
        }
        return;
      }
      // The list is on top: Esc closes it, and the shortcuts of the screen underneath wait until it is closed.
      // A right-click menu over the list owns Esc and Tab until it closes (ui/contextmenu.ts).
      if (e.defaultPrevented || document.querySelector('.cmenu:not(.leaving)')) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopImmediatePropagation();
        hide(true);
      } else if (e.key === 'Tab') {
        // Tab stays in the list: the shell's own focus traps underneath never see it.
        keepFocusIn(e, dialog);
        e.stopImmediatePropagation();
      }
      else if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    };
    window.addEventListener('keydown', onKey, true);
    onCleanup(() => window.removeEventListener('keydown', onKey, true));
  });

  const rows = (group: Shortcut['group']) => Object.values(SHORTCUTS as Record<string, Shortcut>).filter((s) => s.group === group);
  return (
    <Show when={shown()}>
      <div class="pal-scrim" classList={{ leaving: !open() }} inert={!open()} onPointerDown={(e) => e.target === e.currentTarget && hide(true)}>
        <div class="pal keys-sheet" role="dialog" aria-modal="true" aria-labelledby="keys-title" ref={(el) => (dialog = el)}>
          <div class="keys-top">
            <h2 id="keys-title">{t('keys.title')}</h2>
            <button type="button" class="iconbtn" aria-label={t('window.close')} title={t('window.close')} onClick={() => hide(true)} ref={(el) => queueMicrotask(() => el.focus())}>
              <Icon name="close" />
            </button>
          </div>
          <div class="pal-list">
            <For each={GROUPS}>
              {([group, heading]) => (
                <section aria-label={t(heading)}>
                  <div class="pal-head" aria-hidden="true">{t(heading)}</div>
                  <dl>
                    <For each={rows(group)}>
                      {(s) => (
                        <div class="keys-row">
                          <dt>{t(s.label)}</dt>
                          <dd>
                            <For each={s.caps}>{(c) => <kbd>{cap(c)}</kbd>}</For>
                            <Show when={s.also}>
                              {(also) => (
                                <>
                                  <span class="keys-or">{t('keys.or')}</span>
                                  <For each={also()}>{(c) => <kbd>{cap(c)}</kbd>}</For>
                                </>
                              )}
                            </Show>
                          </dd>
                        </div>
                      )}
                    </For>
                  </dl>
                </section>
              )}
            </For>
          </div>
        </div>
      </div>
    </Show>
  );
}
