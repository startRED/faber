import { For, Show } from 'solid-js';
import { idleLife, slowTick, windowActive } from './idle.ts';

/** How long a hint stays before the next one fades in. */
const HINT_MS = 8000;

/** What an empty composer shows while nobody types: a calm typing bar that blinks and a hint that changes every few
 * seconds. The bar is drawn only while the real caret is not (the field has no focus, or the window is not in front), so
 * the two are never on screen together. Reduced motion and no motion, and a hidden window, render nothing: the field's
 * own placeholder stays. The words are decoration; the field keeps its placeholder for readers of the screen. */
export function Ghost(props: { hints: string[]; focused: boolean }) {
  return (
    <Show when={idleLife() && props.hints.length > 0}>
      <Hints hints={props.hints} focused={props.focused} />
    </Show>
  );
}

function Hints(props: { hints: string[]; focused: boolean }) {
  const tick = slowTick(HINT_MS, () => props.hints.length > 1);
  const shown = () => tick() % props.hints.length;
  return (
    <div class="idle-box" aria-hidden="true">
      <Show when={!props.focused || !windowActive()}>
        <i class="g-caret" />
      </Show>
      <For each={props.hints}>{(hint, i) => <span class="g-hint" classList={{ on: i() === shown() }}>{hint}</span>}</For>
    </div>
  );
}
