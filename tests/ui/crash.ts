// Test builds only. The `ui` suite waits for the page to report; when the page breaks before its checks finish (a script
// error, an API this webview lacks) nothing would ever arrive and the run would only say "no checks report". This turns
// the first such error into the report itself, so the failure names what broke. Loaded before the checks, so a failure
// while the checks load is caught too.

import { invoke } from '@tauri-apps/api/core';
import type { Result } from './check.ts';

let reported = false;

/** Sends the report once; the shell writes it to `FABER_CHECKS_OUT` and ends the app. */
export function report(results: Result[]): void {
  if (reported) return;
  reported = true;
  clearInterval(beat);
  void invoke('report_checks', { json: JSON.stringify(results) });
}

let note = '';
let since = 0;
let beat: ReturnType<typeof setInterval> | undefined;

/** Tells the shell which check starts now; it writes a note the runner watches, so a slow run is never taken for a stuck one. */
export function progress(index: number, total: number, name: string): void {
  note = `${index + 1}/${total} ${name}`;
  since = Date.now();
  void invoke('checks_progress', { note }).catch(() => {});
  // A heartbeat the runner never counts as progress (`BEAT` in testkit/src/checks.rs): when the checks stop, it tells a
  // page whose timers stopped (no beat) from a page that runs while its checks do not move (beats).
  beat ??= setInterval(() => {
    void invoke('checks_progress', { note: `${note} · page alive ${Math.round((Date.now() - since) / 1000)} s` }).catch(() => {});
  }, 10_000);
}

export function crash(message: string): void {
  report([{ name: 'page error', ok: false, message, ms: 0 }]);
}

if (new URLSearchParams(location.search).has('checks')) {
  // Only errors that carry an `Error`: the harmless "ResizeObserver loop" notice does not.
  window.addEventListener('error', (e) => {
    if (e.error !== undefined && e.error !== null) crash(`${e.error instanceof Error ? e.error.stack || e.error.message : String(e.error)}`);
  });
  window.addEventListener('unhandledrejection', (e) => crash(`unhandled rejection: ${e.reason instanceof Error ? e.reason.stack || e.reason.message : String(e.reason)}`));
}
