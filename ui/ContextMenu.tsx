import { createEffect, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { closeMenu, type MenuItem, menu, type OpenMenu, unavailable } from './contextmenu.ts';
import { t } from './i18n.ts';
import { keepForExit } from './motion.ts';

/** The time after arming an item in which a second activation still belongs to the same gesture (the system's double-click
 * interval): the confirming activation must be a separate, deliberate one. */
const SAME_GESTURE = 500;

function Panel(props: { m: OpenMenu; leaving: boolean }) {
  const [armed, setArmed] = createSignal<string>();
  let menuEl: HTMLDivElement | undefined;
  let armedAt = 0;
  const buttons = () => [...(menuEl?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
  onMount(() => {
    const el = menuEl!;
    // Opens at the pointer, flips above it near the bottom edge and always stays inside the window.
    // The layout size, not the drawn one: the entrance has already scaled the menu a little.
    const [width, height] = [el.offsetWidth, el.offsetHeight];
    const pad = 8;
    const flip = props.m.y + height > innerHeight - pad;
    el.style.left = `${Math.max(pad, Math.min(props.m.x, innerWidth - width - pad))}px`;
    el.style.top = `${Math.max(pad, flip ? props.m.y - height : props.m.y)}px`;
    el.style.transformOrigin = `left ${flip ? 'bottom' : 'top'}`;
    (props.m.keyboard ? buttons()[0] : el)?.focus({ preventScroll: true });
  });
  const move = (to: (all: HTMLButtonElement[], at: number) => number) => {
    const all = buttons();
    all[to(all, all.indexOf(document.activeElement as HTMLButtonElement))]?.focus();
  };
  const choose = (item: MenuItem) => {
    if (unavailable(item)) return;
    if (item.confirm && armed() !== item.id) {
      armedAt = performance.now();
      return setArmed(item.id);
    }
    if (item.confirm && performance.now() - armedAt < SAME_GESTURE) return;
    closeMenu(true);
    item.run();
  };
  return (
    <div
      ref={(node) => (menuEl = node)}
      class="cmenu"
      classList={{ leaving: props.leaving }}
      role="menu"
      tabindex="-1"
      aria-label={t('menu.label')}
      inert={props.leaving}
      onMouseLeave={() => setArmed(undefined)}
      onKeyDown={(e) => {
        if (e.key === 'ArrowDown') move((all, at) => (at + 1) % all.length);
        else if (e.key === 'ArrowUp') move((all, at) => (at <= 0 ? all.length - 1 : at - 1));
        else if (e.key === 'Home') move(() => 0);
        else if (e.key === 'End') move((all) => all.length - 1);
        else if (e.key === 'Tab') closeMenu(true);
        else return;
        e.preventDefault();
      }}
    >
      <For each={props.m.items}>
        {(item) => (
          <button
            type="button"
            role="menuitem"
            class="cm-item"
            classList={{ danger: item.danger, armed: armed() === item.id }}
            tabindex="-1"
            disabled={unavailable(item)}
            onFocusOut={() => armed() === item.id && setArmed(undefined)}
            onClick={() => choose(item)}
          >
            {armed() === item.id ? item.confirm : item.label}
          </button>
        )}
      </For>
    </div>
  );
}

// Only the first host that is mounted draws, so a second one (a screen check mounts its own) never doubles the menu.
const hosts: symbol[] = [];
const [first, setFirst] = createSignal<symbol>();

/** The one open right-click menu, drawn above everything. It leaves with the normal exit and, in reduced or no motion, at once. */
export function ContextMenuHost() {
  const me = Symbol();
  hosts.push(me);
  setFirst(hosts[0]);
  onCleanup(() => {
    hosts.splice(hosts.indexOf(me), 1);
    setFirst(hosts[0]);
  });
  const shown = keepForExit(() => menu() !== undefined);
  // The last menu stays while it fades out.
  const [held, setHeld] = createSignal<OpenMenu>();
  createEffect(() => {
    const open = menu();
    if (open) setHeld(open);
  });
  return (
    <Show when={shown() && first() === me ? held() : undefined} keyed>
      {(m) => <Panel m={m} leaving={menu() !== m} />}
    </Show>
  );
}
