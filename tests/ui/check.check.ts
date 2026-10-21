import { check, deepEqual, doesNotMatch, equal, finished, frames, matches, notEqual, ok } from './check.ts';

// An assertion that never throws would turn every other check green, so the helpers are checked first.
const throws = (run: () => void) => {
  try {
    run();
  } catch {
    return true;
  }
  return false;
};

check('check: the assertions fail when they should and pass when they should', () => {
  ok(throws(() => ok(0)), 'ok(0) must throw');
  ok(throws(() => ok('')), "ok('') must throw");
  ok(!throws(() => ok(1)), 'ok(1) must not throw');
  ok(throws(() => equal(1, 2)), 'equal(1, 2) must throw');
  ok(!throws(() => equal('a', 'a')), "equal('a', 'a') must not throw");
  ok(throws(() => notEqual(1, 1)), 'notEqual(1, 1) must throw');
  ok(!throws(() => notEqual(1, 2)), 'notEqual(1, 2) must not throw');
  ok(throws(() => deepEqual({ a: [1, 2] }, { a: [1, 3] })), 'deepEqual must see a different array item');
  ok(throws(() => deepEqual({ a: 1 }, { a: 1, b: undefined })), 'deepEqual must see an extra key');
  ok(throws(() => deepEqual([1], { 0: 1 })), 'an array is not an object');
  const hole: unknown[] = [];
  hole.length = 1;
  ok(throws(() => deepEqual(hole, [])), 'an array with a hole is longer than an empty one');
  ok(throws(() => deepEqual(['none', undefined], ['none'])), 'an array with a longer length differs');
  const holey: unknown[] = ['none'];
  holey.length = 2;
  ok(throws(() => deepEqual(holey, ['none', undefined])), 'a hole is not undefined');
  ok(!throws(() => deepEqual([], [])), 'two empty arrays are equal');
  ok(!throws(() => deepEqual({ a: [1, { b: 2 }] }, { a: [1, { b: 2 }] })), 'deepEqual must accept equal structures');
  ok(throws(() => matches('abc', /x/)), 'matches must throw when the text differs');
  ok(!throws(() => matches('abc', /b/)), 'matches must pass when the text matches');
  ok(throws(() => doesNotMatch('abc', /b/)), 'doesNotMatch must throw when the text matches');
  ok(!throws(() => doesNotMatch('abc', /x/)), 'doesNotMatch must pass when it does not');
});

check('check: the frame and animation waits are bounded, so a held-back window cannot hang the run', async () => {
  const held = document.body.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 1000 });
  held.pause();
  const began = performance.now();
  await finished([held], 50);
  held.cancel();
  ok(performance.now() - began < 900, 'a paused animation does not hold the wait past its limit');
  await frames(2, 50);
  ok(performance.now() - began < 1500, 'frames end by themselves');
});
