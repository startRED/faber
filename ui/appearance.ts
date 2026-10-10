import '@fontsource-variable/jetbrains-mono';
import '@fontsource-variable/mona-sans';

// Settings > Appearance (docs/design.md). A theme is a plain object, so it can be saved, shared and offered by the catalog.
export interface Appearance {
  mode: 'dark' | 'black' | 'light' | 'system';
  accent: keyof typeof ACCENTS;
  custom: string | null; // any color the user picked, instead of a named accent
  ui: keyof typeof UI;
  read: keyof typeof READ | 'same';
  density: 'comfortable' | 'compact';
  corners: 'round' | 'square';
}

const MODES = {
  dark: { bg: '#15171b', side: '#111316', surface: '#1d2025', surface2: '#181b1f', line: '#262a30', fg: '#e4e6ea', soft: '#b8bcc4', muted: '#7c828c' },
  black: { bg: '#000000', side: '#050505', surface: '#121214', surface2: '#0a0a0b', line: '#1e1f22', fg: '#ececef', soft: '#b3b6bc', muted: '#76797f' },
  light: { bg: '#f6f7f9', side: '#eef0f3', surface: '#ffffff', surface2: '#fbfbfc', line: '#dfe2e8', fg: '#16181d', soft: '#3a3f48', muted: '#6a707b' },
};
// [on dark, on light]
export const ACCENTS = {
  mono: ['#d0d3d9', '#3a3f48'],
  sage: ['#7fb8a6', '#2f7a64'],
  cobalt: ['#7f9cff', '#2b4ad6'],
  amber: ['#e8b04a', '#9a6200'],
  rose: ['#e98aa6', '#b23a62'],
  violet: ['#b49cff', '#6a45d6'],
  teal: ['#5cc8d6', '#11788a'],
};
const UI = {
  geistmono: "'Geist Mono Variable', ui-monospace, Consolas, monospace",
  jetbrains: "'JetBrains Mono Variable', ui-monospace, Consolas, monospace",
  sans: "'Geist Variable', system-ui, 'Segoe UI', sans-serif",
};
const READ = {
  geist: "'Geist Variable', system-ui, 'Segoe UI', sans-serif",
  mona: "'Mona Sans Variable', system-ui, 'Segoe UI', sans-serif",
};

export const PRESETS: Record<'mono' | 'studio' | 'cockpit', Appearance> = {
  mono: { mode: 'dark', accent: 'mono', custom: null, ui: 'geistmono', read: 'geist', density: 'comfortable', corners: 'round' },
  studio: { mode: 'light', accent: 'cobalt', custom: null, ui: 'sans', read: 'mona', density: 'comfortable', corners: 'square' },
  cockpit: { mode: 'black', accent: 'amber', custom: null, ui: 'jetbrains', read: 'same', density: 'compact', corners: 'square' },
};

export const toAppearance = (saved: Record<string, unknown> | undefined): Appearance => ({ ...PRESETS.mono, ...(saved as Partial<Appearance>) });
export const presetOf = (a: Appearance) => (Object.keys(PRESETS) as (keyof typeof PRESETS)[]).find((k) => JSON.stringify(PRESETS[k]) === JSON.stringify(a));

const systemLight = matchMedia('(prefers-color-scheme: light)');

export function applyAppearance(a: Appearance): void {
  const mode = a.mode === 'system' ? (systemLight.matches ? 'light' : 'dark') : a.mode;
  const root = document.documentElement.style;
  for (const [k, v] of Object.entries(MODES[mode])) root.setProperty(`--${k}`, v);
  root.setProperty('--accent', a.custom ?? ACCENTS[a.accent][mode === 'light' ? 1 : 0]);
  root.setProperty('--ui', UI[a.ui]);
  root.setProperty('--read', a.read === 'same' ? UI[a.ui] : READ[a.read]);
  root.setProperty('--pad', a.density === 'compact' ? '0.7' : '1');
  root.setProperty('--radius', a.corners === 'square' ? '3px' : '10px');
  root.colorScheme = mode === 'light' ? 'light' : 'dark';
}

// "Follow system" changes with the operating system while the app is open.
export function followSystem(get: () => Appearance): () => void {
  const onChange = () => get().mode === 'system' && applyAppearance(get());
  systemLight.addEventListener('change', onChange);
  return () => systemLight.removeEventListener('change', onChange);
}
