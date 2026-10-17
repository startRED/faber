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
  up: ['m5 12 7-7 7 7', 'M12 19V5'],
  clip: ['m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48'],
  search: ['M3 11a8 8 0 1 0 16 0 8 8 0 1 0-16 0', 'm21 21-4.3-4.3'],
  // The rays and the disc of "let the AI choose".
  auto: ['M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1', 'M9 12a3 3 0 1 0 6 0 3 3 0 1 0-6 0'],
  panel: ['M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z', 'M15 3v18'],
  sheet: ['M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z', 'M14 2v4a2 2 0 0 0 2 2h4', 'M8 13h2M14 13h2M8 17h2M14 17h2'],
  close: ['M18 6 6 18', 'm6 6 12 12'],
  // The actions under an answer: copy, try again, and the terminal of the CLI.
  copy: ['M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2Z', 'M4 16V6a2 2 0 0 1 2-2h10'],
  retry: ['M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8', 'M3 3v5h5'],
  terminal: ['m4 17 6-6-6-6', 'M12 19h8'],
  menu: ['M4 6h16', 'M4 12h16', 'M4 18h16'],
  sliders: ['M20 7h-9', 'M14 17H5', 'M17 14a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z', 'M7 4a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z'],
  // The sections of Settings, the lock of the colours that cannot change, and the two ends of a theme file.
  user: ['M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2', 'M12 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z'],
  palette: [
    'M12 22a10 10 0 1 1 10-10 5 5 0 0 1-5 5h-2.2a1.8 1.8 0 0 0-1.4 2.9l.3.4a1.8 1.8 0 0 1-1.4 2.7Z',
    'M13.5 6.5h.01',
    'M17.5 10.5h.01',
    'M8.5 7.5h.01',
    'M6.5 12.5h.01',
  ],
  plug: ['M12 22v-5', 'M9 8V2', 'M15 8V2', 'M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z'],
  gauge: ['m12 14 4-4', 'M3.3 19a10 10 0 1 1 17.4 0'],
  shield: ['M20 13c0 5-3.5 7.5-7.7 8.9a1 1 0 0 1-.6 0C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.2-2.7a1.2 1.2 0 0 1 1.6 0C14.5 3.8 17 5 19 5a1 1 0 0 1 1 1Z'],
  lock: ['M7 11V7a5 5 0 0 1 10 0v4', 'M5 11h14a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2Z'],
  download: ['M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4', 'm7 10 5 5 5-5', 'M12 15V3'],
  upload: ['M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4', 'm17 8-5-5-5 5', 'M12 3v12'],
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
