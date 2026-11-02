// What the shell under test cannot do yet, so the checks that need it wait instead of failing for a reason that is not
// the screen's. A gap is listed only while the shell answers it as missing; the moment a shell supports it, its checks run.

import { api } from '../../ui/api.ts';

/** Starts of the names of the checks that open real native pages for visual answers. */
const VISUAL_PAGES = ['visual answers:', 'visual files: discovery has no source', 'Mermaid: a controlled renderer'];
/** Starts of the names of the checks that drive the browser pane's native page. */
const BROWSER_PANE = ['page in the browser:', 'screen sizes:', 'browser navigation:', 'sign-ins from Firefox:'];

/** True when the shell says it has no window to do `ask` in: the Electron shell's host answers so for what is not built. */
const missing = (ask: () => Promise<unknown>): Promise<boolean> => ask().then(() => false, (error: unknown) => error === 'error.host.noWindow');

/**
 * The checks to leave out of this run. The Electron shell has no page server for visual answers and no browser pane yet
 * (R459: its host answers `error.host.noWindow`); Tauri's shell has both.
 */
export async function shellGaps(): Promise<string[]> {
  // Only the Electron shell has gaps, and Tauri's run asks it nothing (no lease or call the checks could count).
  if (!('faberShell' in globalThis)) return [];
  const gaps: string[] = [];
  if (await missing(() => api.visualOpen().then((id) => api.visualClose(id)))) gaps.push(...VISUAL_PAGES);
  if (await missing(() => api.siteSize(false))) gaps.push(...BROWSER_PANE);
  return gaps;
}
