import type { JSX } from 'solid-js';
import { SCENES, type SceneName } from './scenes.tsx';
import './scenes.css';

export type { SceneName };
export { SCENE_NAMES } from './scenes.tsx';

/**
 * A small drawing that says at a glance what a screen is about (docs/design/scenes.md): a plug that is not connected, a
 * folder to open, an empty shelf. Inline paths in the colour of the text around them (stroke `currentColor`), never an image
 * file, never animated, and always hidden from readers: the words beside it say the same thing.
 */
export function Scene(props: { name: SceneName; class?: string }): JSX.Element {
  const scene = () => SCENES[props.name];
  return (
    <svg class={`scene${props.class ? ` ${props.class}` : ''}`} data-scene={props.name} viewBox={scene().viewBox} width={scene().width} height={scene().height} aria-hidden="true">
      {scene().draw()}
    </svg>
  );
}
