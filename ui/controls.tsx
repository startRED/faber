import { createSignal, createUniqueId, For, type JSX, onCleanup, onMount, Show } from 'solid-js';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { keepForExit } from './motion.ts';

// What Settings (and the first run) are made of: a card per group, one row per option with its name and one line at the left
// and its control at the right, and the longer text of an option behind a small "?" next to its name.

export type Option<T> = [T, string];

/** Choices that exclude each other. The one that is on says so with `aria-pressed`. With `bare` it is only the buttons,
 * for a row that already carries the name; otherwise the name sits above them. */
export function Segmented<T extends string | number>(props: { id: string; label: string; value: T | undefined; options: Option<T>[]; onPick: (v: T) => void; bare?: boolean }) {
  const buttons = (
    <For each={props.options}>
      {([v, text]) => (
        <button type="button" aria-pressed={v === props.value} onClick={() => props.onPick(v)}>
          {text}
        </button>
      )}
    </For>
  );
  return (
    <Show
      when={props.bare}
      fallback={
        <div class="ctl">
          <span class="lbl" id={props.id}>
            {props.label}
          </span>
          <div class="seg-ctl" role="group" aria-labelledby={props.id}>
            {buttons}
          </div>
        </div>
      }
    >
      <div class="seg-ctl" id={props.id} role="group" aria-label={props.label}>
        {buttons}
      </div>
    </Show>
  );
}

/** What opens and closes under a row: it grows from nothing and shrinks back, and is not in the page once it is closed. */
export function Collapse(props: { open: boolean; children: JSX.Element }) {
  const shown = keepForExit(() => props.open);
  return (
    <Show when={shown()}>
      <div class="fold collapse" classList={{ open: props.open }} inert={!props.open}>
        <div class="fold-inner">{props.children}</div>
      </div>
    </Show>
  );
}

/** A group of options: an optional title, then one card. */
export function Group(props: { title?: string; children: JSX.Element }) {
  const id = createUniqueId();
  return (
    <section class="st-group" aria-labelledby={props.title ? id : undefined}>
      <Show when={props.title}>
        <h3 class="st-title" id={id}>
          {props.title}
        </h3>
      </Show>
      <div class="st-card">{props.children}</div>
    </section>
  );
}

/** One option. The name and its one line are at the left, the control at the right. `more` is the long text, one string or a
 * few paragraphs: it unfolds under the row from the "?" and is not in the page while closed. */
export function Opt(props: { id: string; label: string; hint?: string; more?: string | string[]; htmlFor?: string; children?: JSX.Element }) {
  const [open, setOpen] = createSignal(false);
  const paragraphs = () => (Array.isArray(props.more) ? props.more : props.more ? [props.more] : []);
  return (
    <div class="st-opt" data-opt={props.id}>
      <div class="st-text">
        <span class="st-name">
          <Show when={props.htmlFor} fallback={<span id={`${props.id}-name`}>{props.label}</span>}>
            <label id={`${props.id}-name`} for={props.htmlFor}>
              {props.label}
            </label>
          </Show>
          <Show when={paragraphs().length > 0}>
            <button
              type="button"
              class="st-q"
              id={`${props.id}-q`}
              aria-label={t('set.learnMore')}
              aria-describedby={`${props.id}-name`}
              aria-expanded={open()}
              aria-controls={`${props.id}-more`}
              onClick={() => setOpen(!open())}
            >
              ?
            </button>
          </Show>
        </span>
        <Show when={props.hint}>
          <span class="st-hint" id={`${props.id}-hint`}>
            {props.hint}
          </span>
        </Show>
      </div>
      <Show when={props.children}>
        <div class="st-ctl">{props.children}</div>
      </Show>
      <Collapse open={open() && paragraphs().length > 0}>
        <div class="st-more" id={`${props.id}-more`} role="note">
          <For each={paragraphs()}>{(text) => <p class="hint">{text}</p>}</For>
        </div>
      </Collapse>
    </div>
  );
}

/** A switch is a native checkbox. It moves before its change is answered; when the answer is `false` (a refused save) it
 * goes back to where `checked` says it is. */
export function Toggle(props: { id: string; label: string; hint?: string; more?: string | string[]; checked: boolean; disabled?: boolean; onChange: (on: boolean) => void | Promise<boolean> }) {
  return (
    <Opt id={props.id} label={props.label} hint={props.hint} more={props.more} htmlFor={`${props.id}-switch`}>
      <input
        id={`${props.id}-switch`}
        class="sw2"
        type="checkbox"
        role="switch"
        aria-describedby={props.hint ? `${props.id}-hint` : undefined}
        checked={props.checked}
        disabled={props.disabled}
        onChange={async (e) => {
          const box = e.currentTarget;
          if ((await props.onChange(box.checked)) === false) box.checked = props.checked;
        }}
      />
    </Opt>
  );
}

/** A folded group inside a card: its title is the button, and what is inside is not in the page while it is closed. */
export function Fold(props: { id: string; title: string; hint?: string; children: JSX.Element }) {
  const [open, setOpen] = createSignal(false);
  return (
    <div class="st-fold" data-fold={props.id}>
      <button type="button" class="st-fold-head" id={props.id} aria-expanded={open()} aria-controls={`${props.id}-body`} onClick={() => setOpen(!open())}>
        <span class="st-text">
          <span class="st-name">{props.title}</span>
          <Show when={props.hint}>
            <span class="st-hint">{props.hint}</span>
          </Show>
        </span>
        <Icon name={open() ? 'down' : 'right'} small />
      </button>
      <Collapse open={open()}>
        <div class="st-fold-body" id={`${props.id}-body`}>
          {props.children}
        </div>
      </Collapse>
    </div>
  );
}

/** A select whose open menu shows each choice with its own line. Focus stays on the button (a select-only combobox); the
 * arrows move, Enter or Space picks, and Esc closes the menu without leaving Settings. */
export function PickMenu<T extends string>(props: { id: string; label: string; value: T; options: [T, string, string][]; onPick: (v: T) => void }) {
  const [open, setOpen] = createSignal(false);
  const [active, setActive] = createSignal(0);
  // The menu stays in the page for its short exit, and only in full motion.
  const shown = keepForExit(open);
  let root: HTMLDivElement | undefined;
  let button: HTMLButtonElement | undefined;
  const at = () => Math.max(0, props.options.findIndex(([v]) => v === props.value));
  const name = () => props.options[at()]?.[1] ?? '';
  const show = () => {
    setActive(at());
    setOpen(true);
  };
  const hide = () => {
    setOpen(false);
    button?.focus();
  };
  const pick = (index: number) => {
    const choice = props.options[index];
    if (choice) props.onPick(choice[0]);
    hide();
  };
  const move = (step: number) => setActive((i) => Math.min(props.options.length - 1, Math.max(0, i + step)));
  onMount(() => {
    const away = (e: PointerEvent) => {
      if (open() && root && !root.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', away);
    onCleanup(() => document.removeEventListener('pointerdown', away));
  });
  const onKey = (e: KeyboardEvent) => {
    const keys = ['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' ', 'Escape', 'Tab'];
    if (!keys.includes(e.key)) return;
    if (e.key === 'Tab') return setOpen(false);
    // Esc belongs to the menu while it is open: it must not close Settings behind it.
    if (e.key === 'Escape') {
      if (!open()) return;
      e.stopPropagation();
      return hide();
    }
    e.preventDefault();
    if (!open()) return show();
    if (e.key === 'ArrowDown') move(1);
    else if (e.key === 'ArrowUp') move(-1);
    else if (e.key === 'Home') setActive(0);
    else if (e.key === 'End') setActive(props.options.length - 1);
    else pick(active());
  };
  return (
    <div class="st-pick" ref={(el) => (root = el)} on:keydown={onKey}>
      <button
        type="button"
        class="pill st-pick-btn"
        id={props.id}
        ref={(el) => (button = el)}
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open()}
        aria-controls={`${props.id}-list`}
        aria-activedescendant={open() ? `${props.id}-opt-${active()}` : undefined}
        aria-label={props.label}
        onClick={() => (open() ? setOpen(false) : show())}
      >
        <span>{name()}</span>
        <Icon name="down" small />
      </button>
      <Show when={shown()}>
        <div class="st-menu" classList={{ leaving: !open() }} inert={!open()} id={`${props.id}-list`} role="listbox" aria-label={props.label}>
          <For each={props.options}>
            {([v, text, line], i) => (
              <div
                class="st-menu-opt"
                id={`${props.id}-opt-${i()}`}
                role="option"
                aria-selected={v === props.value}
                classList={{ active: i() === active() }}
                onPointerEnter={() => setActive(i())}
                onClick={() => pick(i())}
              >
                <b>{text}</b>
                <small>{line}</small>
                <Show when={v === props.value}>
                  <Icon name="check" small />
                </Show>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
