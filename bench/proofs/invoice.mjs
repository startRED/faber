import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [folder, pristine] = process.argv.slice(2), checks = [];
const check = (name, run) => {
  try { run(); checks.push({ name, ok: true }); }
  catch (error) { checks.push({ name, ok: false, detail: String(error.message).slice(0, 500) }); }
};
check('README preserved', () => {
  if (!readFileSync(join(folder, 'README.md')).equals(readFileSync(join(pristine, 'README.md')))) throw Error('README changed');
});
check('invoice contract and invalid inputs', () => {
  const code = `import assert from 'node:assert/strict';
    const { total } = await import(${JSON.stringify(pathToFileURL(resolve(folder, 'invoice.mjs')).href)});
    assert.equal(total([]), 0);
    assert.equal(total([{quantity: 2, unitCents: 123}, {quantity: 0, unitCents: 999}, {quantity: 3, unitCents: 0}]), 246);
    assert.equal(total([{quantity: 7, unitCents: 19}, {quantity: 1, unitCents: 1}]), 134);
    for (const bad of [-1, 0.5, '2', null, undefined, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      assert.throws(() => total([{quantity: bad, unitCents: 100}]));
      assert.throws(() => total([{quantity: 1, unitCents: bad}]));
    }`;
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', code], { cwd: folder, windowsHide: true, encoding: 'utf8', timeout: 15000 });
  if (result.status !== 0) throw Error(result.error?.message ?? result.stderr ?? 'invoice failed');
});
check('a runnable regression test was supplied', () => {
  // Nested proof checks must start a fresh runner, not inherit Node's test-child marker.
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  const result = spawnSync(process.execPath, ['--test', '--test-reporter=tap'], { cwd: folder, env, windowsHide: true, encoding: 'utf8', timeout: 15000 });
  if (result.status !== 0) throw Error(result.error?.message ?? result.stdout ?? 'regression test failed');
  if (!/^# tests [1-9]\d*\r?$/m.test(result.stdout)) throw Error('No regression tests ran');
});
console.log(JSON.stringify({ pass: checks.every(c => c.ok), checks }));
