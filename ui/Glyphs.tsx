import { For } from 'solid-js';
import type { Vendor } from './api.ts';

// The small drawings of the shell, from the approved mock: thin lines in the colour of the text around them. They are
// decoration; the words next to them say what they mean.
const PATHS = {
  chat: ['M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z'],
  folder: ['M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z'],
  plus: ['M5 12h14', 'M12 5v14'],
  keyboard: ['M4 6h16a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2Z', 'M6 10h.01', 'M10 10h.01', 'M14 10h.01', 'M18 10h.01', 'M8 14h8'],
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
  // An opened subagent: fill the stage as a full thread, and back to the column.
  expand: ['m15 3 6 0 0 6', 'M21 3l-7 7', 'm9 21-6 0 0-6', 'M3 21l7-7'],
  shrink: ['m4 14 6 0 0 6', 'M10 14l-7 7', 'm20 10-6 0 0-6', 'M14 10l7-7'],
  // The actions under an answer: copy, try again, and the terminal of the CLI.
  copy: ['M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2Z', 'M4 16V6a2 2 0 0 1 2-2h10'],
  loop: ['m17 2 4 4-4 4', 'M3 11v-1a4 4 0 0 1 4-4h14', 'm7 22-4-4 4-4', 'M21 13v1a4 4 0 0 1-4 4H3'],
  retry: ['M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8', 'M3 3v5h5'],
  terminal: ['m4 17 6-6-6-6', 'M12 19h8'],
  menu: ['M4 6h16', 'M4 12h16', 'M4 18h16'],
  more: ['M4 12a1 1 0 1 0 2 0 1 1 0 0 0-2 0', 'M11 12a1 1 0 1 0 2 0 1 1 0 0 0-2 0', 'M18 12a1 1 0 1 0 2 0 1 1 0 0 0-2 0'],
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
  // Under a message: read aloud, stop reading, edit, and the arrow that takes the thread to its end.
  speaker: ['M11 5 6 9H2v6h4l5 4V5Z', 'M15.5 8.5a5 5 0 0 1 0 7', 'M19 5a9 9 0 0 1 0 14'],
  stop: ['M7 7h10v10H7Z'],
  edit: ['M12 20h9', 'M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z'],
  end: ['M12 5v14', 'm19 12-7 7-7-7', 'M5 21h14'],
  // The buttons of the topbar that pick what the right column shows: the conversation's changes and Faber's browser.
  changes: ['M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z', 'M9 11h6', 'M12 8v6', 'M9 17h6'],
  // The page in the person's own browser, out of Faber.
  external: ['M15 3h6v6', 'M10 14 21 3', 'M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6'],
  globe: ['M3 12a9 9 0 1 0 18 0 9 9 0 1 0-18 0', 'M3 12h18', 'M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.6-3.8-9s1.3-6.4 3.8-9Z'],
  // The screen a page of Faber's browser is drawn at.
  monitor: ['M4 4h16a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z', 'M8 21h8', 'M12 17v4'],
  // The jobs of the catalog: one drawing per kind of tool, beside its name.
  bug: ['M8 10a4 4 0 0 1 8 0v4a4 4 0 0 1-8 0Z', 'M12 10v8', 'M4 13h4M16 13h4', 'm5 8 3 2M19 8l-3 2', 'm5 19 3-2M19 19l-3-2', 'M10 6 9 4M14 6l1-2'],
  flask: ['M9 3h6', 'M10 3v6l-5.5 9.5A1.7 1.7 0 0 0 6 21h12a1.7 1.7 0 0 0 1.5-2.5L14 9V3', 'M7.5 15h9'],
  feather: ['M20 12.5a6 6 0 0 0-8.5-8.5L5 10.5V19h8.5Z', 'M16 8 3 21', 'M17.5 15H9'],
  layers: ['m12 3 9 4.5-9 4.5-9-4.5Z', 'm3 12 9 4.5 9-4.5', 'm3 16.5 9 4.5 9-4.5'],
  list: ['M10 6h10', 'M10 12h10', 'M10 18h10', 'm4 6 1.5 1.5L8 5', 'm4 12 1.5 1.5L8 11', 'm4 18 1.5 1.5L8 17'],
  wand: ['m3 21 10-10', 'M15 3v3M15 12v3M10 7.5h3M17 7.5h3', 'm11.5 4.5 1.5 1.5M17 9l1.5 1.5M18.5 4.5 17 6'],
  database: ['M4 6a8 3 0 1 0 16 0 8 3 0 1 0-16 0', 'M4 6v12a8 3 0 0 0 16 0V6', 'M4 12a8 3 0 0 0 16 0'],
  card: ['M4 5h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z', 'M2 10h20', 'M6 15h4'],
  cloud: ['M17.5 19H8a6 6 0 1 1 5.7-8h3.8a4 4 0 0 1 0 8Z'],
  phone: ['M8 2h8a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2Z', 'M11 18h2'],
  megaphone: ['m3 10 16-5v14L3 14Z', 'M7 15.2V18a2 2 0 0 0 4 0v-1.6'],
  gamepad: ['M7 6h10a5 5 0 0 1 5 5v2a4 4 0 0 1-7 2.6l-.6-.6H9.6l-.6.6A4 4 0 0 1 2 13v-2a5 5 0 0 1 5-5Z', 'M6 11h4M8 9v4', 'M15 10h.01M17 12h.01'],
  dice: ['M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z', 'M8.5 8.5h.01M15.5 8.5h.01M12 12h.01M8.5 15.5h.01M15.5 15.5h.01'],
  puzzle: ['M10 3h4v3a2 2 0 1 0 4 0V3h3v7h-3a2 2 0 1 0 0 4h3v7h-7v-3a2 2 0 1 0-4 0v3H3v-7h3a2 2 0 1 0 0-4H3V3Z'],
} as const;

export type GlyphName = keyof typeof PATHS;

/** The same drawing as markup, for the places that build their DOM by hand (the copy button of a code block). */
export const iconMarkup = (name: keyof typeof PATHS) =>
  `<svg class="i sm" viewBox="0 0 24 24" aria-hidden="true">${PATHS[name].map((d) => `<path d="${d}"/>`).join('')}</svg>`;

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
