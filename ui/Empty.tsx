import type { JSX } from 'solid-js';
import { Show } from 'solid-js';
import { Peek } from './Mark.tsx';

/**
 * The one empty state: the peeking mark, a title in the heading size, one short line and at most one button. It is used
 * for no AI, no project and a project with nothing in it yet (docs/design/brand.md, "Identity"). The line never names a
 * path through Settings: the button goes there. `children` sit under the button, for a list that belongs to the same place.
 */
export function Empty(props: {
  title: string;
  line?: string;
  /** The one button. `restore` is the `data-restore` that gives focus back when the dialog it opens closes. */
  button?: { label: string; onClick: () => void; restore?: string };
  /** Extra class on the root, for the place that holds it. */
  class?: string;
  children?: JSX.Element;
}) {
  return (
    <div class={`empty${props.class ? ` ${props.class}` : ''}`}>
      <Peek />
      <h1 class="display">{props.title}</h1>
      <Show when={props.line}>{(line) => <p>{line()}</p>}</Show>
      <Show when={props.button}>
        {(button) => (
          <button type="button" class="btn primary" data-restore={button().restore} onClick={() => button().onClick()}>
            {button().label}
          </button>
        )}
      </Show>
      {props.children}
    </div>
  );
}
