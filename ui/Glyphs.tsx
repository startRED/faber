import { For } from 'solid-js';
import type { Vendor } from './api.ts';

// The small drawings of the shell, from the approved mock: thin lines in the colour of the text around them. They are
// decoration; the words next to them say what they mean.
const PATHS = {
  chat: ['M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z'],
  folder: ['M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z'],
  plus: ['M5 12h14', 'M12 5v14'],
  // The speech bubble with a question mark: the one shape that means "needs you".
  ask: ['M7.9 20A9 9 0 1 0 4 16.1L2 22Z', 'M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3', 'M12 17h.01'],
  right: ['m9 18 6-6-6-6'],
  left: ['m15 18-6-6 6-6'],
  check: ['M20 6 9 17l-5-5'],
  down: ['m6 9 6 6 6-6'],
  close: ['M18 6 6 18', 'm6 6 12 12'],
  menu: ['M4 6h16', 'M4 12h16', 'M4 18h16'],
  sliders: ['M20 7h-9', 'M14 17H5', 'M17 14a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z', 'M7 4a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z'],
} as const;

export function Icon(props: { name: keyof typeof PATHS; small?: boolean }) {
  return (
    <svg class="i" classList={{ sm: props.small }} viewBox="0 0 24 24" aria-hidden="true">
      <For each={PATHS[props.name]}>{(d) => <path d={d} />}</For>
    </svg>
  );
}

/** A vendor's shape and colour: a square for Claude, a diamond for Codex, a neutral dot while Auto is still choosing. The
 * vendor's name always comes with it. */
export function VendorMark(props: { vendor?: Vendor }) {
  return <span class="mk" classList={{ claude: props.vendor === 'claude', codex: props.vendor === 'codex' }} aria-hidden="true" />;
}
