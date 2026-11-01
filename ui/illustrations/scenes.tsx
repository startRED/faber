import type { JSX } from 'solid-js';
import { MARK } from '../Mark.tsx';

export type SceneName = 'noAi' | 'openProject' | 'emptyProject' | 'noResults' | 'catalogEmpty' | 'accounts';

export const SCENE_NAMES: readonly SceneName[] = ['noAi', 'openProject', 'emptyProject', 'noResults', 'catalogEmpty', 'accounts'];

interface Drawing {
  viewBox: string;
  width: number;
  height: number;
  draw: () => JSX.Element;
}

export const SCENES: Record<SceneName, Drawing> = {
  noAi: {
    viewBox: '0 0 64 44', width: 64, height: 44, draw: () => (
      <g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2 31h5c5 0 6-4 6-8v-5h12v5c0 5-3 9-8 9h-4M25 21h5m-5 6h5M42 15h12v18H42a4 4 0 0 1-4-4V19a4 4 0 0 1 4-4ZM41 21h2m-2 6h2M54 24h4c3 0 4 2 4 5v14M34 15l-1-3m1 21-1 3" />
      </g>
    ),
  },
  openProject: {
    viewBox: '0 0 64 44', width: 64, height: 44, draw: () => (
      <g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M7 37V17a3 3 0 0 1 3-3h12l4 4h27a3 3 0 0 1 3 3v4M7 37l5-12h45l-5 14a5 5 0 0 1-5 4H12a5 5 0 0 1-5-6ZM33 3v17m-5-5 5 5 5-5" />
      </g>
    ),
  },
  emptyProject: {
    viewBox: '0 0 64 44', width: 64, height: 44, draw: () => (
      <g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M9 26V14a3 3 0 0 1 3-3h9l4 4h27a3 3 0 0 1 3 3v8M7 27h50l-4 13a4 4 0 0 1-4 3H15a4 4 0 0 1-4-3L7 27Z" />
        <path stroke-dasharray="2 3" d="M26 25V3h9l5 5v17M35 3v6h5" />
      </g>
    ),
  },
  noResults: {
    viewBox: '0 0 64 44', width: 64, height: 44, draw: () => (
      <g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M16 19a13 13 0 1 0 26 0 13 13 0 0 0-26 0ZM39 29l13 14M23 19h.1m6 0h.1" />
        <path stroke-dasharray="1 2" d="M33 19a2 2 0 1 0 4 0 2 2 0 0 0-4 0Z" />
      </g>
    ),
  },
  catalogEmpty: {
    viewBox: '0 0 64 44', width: 64, height: 44, draw: () => (
      <g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M8 9v33M56 9v33M8 21h48M8 41h48" />
        <path stroke-dasharray="2 3" d="M25 28h14v13H25z" />
      </g>
    ),
  },
  accounts: {
    viewBox: '0 0 320 96', width: 320, height: 96, draw: () => (
      <g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M78 30h21c12 0 17 18 31 18h7M78 66h21c12 0 17-18 31-18M184 48h49" />
        <path style={{ stroke: 'var(--claude-ink)', fill: 'color-mix(in srgb, var(--claude-ink) 18%, transparent)' }} d="M54 19h22v22H54Z" />
        <path style={{ stroke: 'var(--codex-ink)', fill: 'color-mix(in srgb, var(--codex-ink) 18%, transparent)' }} d="M65 54l12 12-12 12-12-12Z" />
        <g transform="translate(138 25) scale(.75)"><path fill="currentColor" fill-rule="evenodd" stroke="none" d={MARK} /></g>
        <path d="M236 31h31a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4h-31a4 4 0 0 1-4-4V35a4 4 0 0 1 4-4ZM232 40h39M238 35h.1m5 0h.1" />
      </g>
    ),
  },
};
