import { For, Show } from 'solid-js';

// The two controls Settings is made of: a group of buttons of which one is on, and a switch.

export type Option<T> = [T, string];

/** Choices that exclude each other. The one that is on says so with `aria-pressed`. */
export function Segmented<T extends string | number>(props: { id: string; label: string; value: T | undefined; options: Option<T>[]; onPick: (v: T) => void }) {
  return (
    <div class="ctl">
      <span class="lbl" id={props.id}>
        {props.label}
      </span>
      <div class="seg-ctl" role="group" aria-labelledby={props.id}>
        <For each={props.options}>
          {([v, text]) => (
            <button type="button" aria-pressed={v === props.value} onClick={() => props.onPick(v)}>
              {text}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

/** A switch is a native checkbox. It moves before its change is answered; when the answer is `false` (a refused save) it
 * goes back to where `checked` says it is. */
export function Toggle(props: { label: string; hint?: string; checked: boolean; onChange: (on: boolean) => void | Promise<boolean> }) {
  return (
    <label class="tg">
      <span>
        {props.label}
        <Show when={props.hint}>
          <small>{props.hint}</small>
        </Show>
      </span>
      <input
        class="sw2"
        type="checkbox"
        role="switch"
        checked={props.checked}
        onChange={async (e) => {
          const box = e.currentTarget;
          if ((await props.onChange(box.checked)) === false) box.checked = props.checked;
        }}
      />
    </label>
  );
}
