import { check, deepEqual, doesNotMatch, equal, finished, frames, matches, notEqual, ok, runAll } from './check.ts';

check('check: a name filter executes only matching bodies and no filter executes every body', async () => {
  let selected = 0, second = 0, other = 0;
  const checks = [
    { name: 'prefix selected suffix', run: () => { selected++; } },
    { name: 'other', run: () => { other++; } },
    { name: 'second selected', run: () => { second++; } },
  ];
  const progress: unknown[] = [];
  const results = await runAll((...args) => { progress.push(args); }, 'selected', checks);
  deepEqual([selected, second, other], [1, 1, 0], 'every selected body runs and unselected bodies never run');
  deepEqual(results.map((result) => result.name), ['prefix selected suffix', 'second selected']);
  ok(results.every((result) => result.ok), 'every selected result must pass');
  deepEqual(progress, [[0, 2, 'prefix selected suffix'], [1, 2, 'second selected']]);
  await runAll(undefined, undefined, checks);
  deepEqual([selected, second, other], [2, 2, 1], 'the default still runs every body');
  deepEqual(await runAll(undefined, 'missing', checks), []);
  deepEqual([selected, second, other], [2, 2, 1], 'no match executes no bodies');
  deepEqual(await runAll(undefined, 'SELECTED', checks), [], 'matching remains case-sensitive');
  await runAll(undefined, '', checks);
  deepEqual([selected, second, other], [3, 3, 2], 'an empty filter still matches every name');
});

// An assertion that never throws would turn every other check green, so the helpers are checked too.
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
