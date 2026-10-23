// A tiny check registry for the screen's own logic. The checks run inside a hidden test build of Faber (see
// testkit/src/checks.rs), so nothing here may use Node. A check throws to fail; the assertions below do that.

import { useLanguage } from '../../ui/i18n.ts';
import { forgetCatalogView } from '../../ui/Catalog.tsx';
import { forgetSection } from '../../ui/Settings.tsx';

export type Result = { name: string; ok: boolean; message: string; ms: number };

const registry: Array<{ name: string; run: () => void | Promise<void> }> = [];

export function check(name: string, run: () => void | Promise<void>): void {
  registry.push({ name, run });
}

/** One check never holds the run for more than this: a promise that never settles (a frame or an animation the webview
 * holds back) fails that check by name and the run goes on, instead of stopping everything until the runner gives up. */
export const CHECK_LIMIT = 60_000;

/** Two frames, but never forever: a window the system holds back (a hidden one on macOS) can pause frames, and a frame
 * awaited without a bound would hang the whole run. What a check needs is asserted after it, by state. */
export const frames = (count = 2, limit = 1000): Promise<void> => {
  const frame = (left: number) => new Promise<void>((done) => requestAnimationFrame(() => (left > 1 ? frame(left - 1).then(done) : done())));
  return Promise.race([frame(count), new Promise<void>((done) => setTimeout(done, limit))]);
};

/** The animations are done, or `limit` passed: `finished` settles in the page's rendering step, which a held-back window skips. */
export const finished = (animations: Animation[], limit = 1500): Promise<unknown> =>
  Promise.race([Promise.all(animations.map((animation) => animation.finished)), new Promise((done) => setTimeout(done, limit))]);

/**
 * Runs matching registered checks, in registration order, and reports each one. `onStart` hears each check just
 * before it runs, so a runner can tell a slow page from a stuck one.
 */
export async function runAll(onStart?: (index: number, total: number, name: string) => void, filter?: string, checks = registry): Promise<Result[]> {
  const results: Result[] = [];
  const selected = checks.filter(({ name }) => filter === undefined || name.includes(filter));
  for (const [index, { name, run }] of selected.entries()) {
    onStart?.(index, selected.length, name);
    // Every check starts in English: the language the system starts with (a Portuguese PC), or one a check before it left, must not decide which pass.
    useLanguage('en');
    // What Faber keeps for the person while it runs must not carry from one check to the next.
    forgetSection(); forgetCatalogView();
    try { localStorage.removeItem('faber.column'); } catch { /* storage may be missing */ }
    const started = performance.now();
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([
        run(),
        new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error(`the check did not finish in ${CHECK_LIMIT / 1000} s`)), CHECK_LIMIT); }),
      ]);
      results.push({ name, ok: true, message: '', ms: performance.now() - started });
    } catch (error) {
      results.push({ name, ok: false, message: error instanceof Error ? error.message : String(error), ms: performance.now() - started });
    } finally {
      clearTimeout(timer);
    }
  }
  return results;
}

const show = (value: unknown): string => {
  if (value === undefined) return 'undefined';
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
};

function same(actual: unknown, expected: unknown): boolean {
  if (Object.is(actual, expected)) return true;
  if (typeof actual !== 'object' || typeof expected !== 'object' || actual === null || expected === null) return false;
  if (Array.isArray(actual) !== Array.isArray(expected)) return false;
  if (Array.isArray(actual) && Array.isArray(expected)) {
    // A hole is not undefined, and a longer array is a different array.
    if (actual.length !== expected.length) return false;
    for (let i = 0; i < actual.length; i++) {
      if ((i in actual) !== (i in expected) || !same(actual[i], expected[i])) return false;
    }
    return true;
  }
  const a = actual as Record<string, unknown>;
  const e = expected as Record<string, unknown>;
  const keys = Object.keys(a);
  return keys.length === Object.keys(e).length && keys.every((key) => key in e && same(a[key], e[key]));
}

export function ok(value: unknown, message = 'expected a true value'): asserts value {
  if (!value) throw new Error(message);
}

export function equal<T>(actual: T, expected: T, message = 'values differ'): void {
  if (!Object.is(actual, expected)) throw new Error(`${message}: expected ${show(expected)}, got ${show(actual)}`);
}

export function notEqual<T>(actual: T, unexpected: T, message = 'values should differ'): void {
  if (Object.is(actual, unexpected)) throw new Error(`${message}: both are ${show(actual)}`);
}

export function deepEqual(actual: unknown, expected: unknown, message = 'values differ'): void {
  if (!same(actual, expected)) throw new Error(`${message}: expected ${show(expected)}, got ${show(actual)}`);
}

export function matches(text: string, pattern: RegExp, message = 'text does not match'): void {
  if (!pattern.test(text)) throw new Error(`${message}: ${pattern} does not match ${show(text)}`);
}

export function doesNotMatch(text: string, pattern: RegExp, message = 'text should not match'): void {
  if (pattern.test(text)) throw new Error(`${message}: ${pattern} matches ${show(text)}`);
}
