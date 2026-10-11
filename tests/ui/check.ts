// A tiny check registry for the screen's own logic. The checks run inside a hidden test build of Faber (see
// testkit/src/checks.rs), so nothing here may use Node. A check throws to fail; the assertions below do that.

export type Result = { name: string; ok: boolean; message: string; ms: number };

const registry: Array<{ name: string; run: () => void | Promise<void> }> = [];

export function check(name: string, run: () => void | Promise<void>): void {
  registry.push({ name, run });
}

/** Runs every registered check, in the order they were registered, and reports each one. */
export async function runAll(): Promise<Result[]> {
  const results: Result[] = [];
  for (const { name, run } of registry) {
    const started = performance.now();
    try {
      await run();
      results.push({ name, ok: true, message: '', ms: performance.now() - started });
    } catch (error) {
      results.push({ name, ok: false, message: error instanceof Error ? error.message : String(error), ms: performance.now() - started });
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
