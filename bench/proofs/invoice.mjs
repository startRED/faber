import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { cpSync, lstatSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [folder, pristine, , scratchRoot] = process.argv.slice(2), checks = [];
let hasDefaultExport = false;
const check = (name, run) => {
  try { run(); checks.push({ name, ok: true }); }
  catch (error) { checks.push({ name, ok: false, detail: String(error.message).slice(0, 500) }); }
};
check('README preserved', () => {
  if (!readFileSync(join(folder, 'README.md')).equals(readFileSync(join(pristine, 'README.md')))) throw Error('README changed');
});
check('invoice contract and invalid inputs', () => {
  const code = `import assert from 'node:assert/strict';
    const invoice = await import(${JSON.stringify(pathToFileURL(resolve(folder, 'invoice.mjs')).href)});
    const { total } = invoice;
    assert.equal(total([]), 0);
    assert.equal(total([{quantity: 2, unitCents: 123}, {quantity: 0, unitCents: 999}, {quantity: 3, unitCents: 0}]), 246);
    assert.equal(total([{quantity: 7, unitCents: 19}, {quantity: 1, unitCents: 1}]), 134);
    for (const bad of [-1, 0.5, '2', null, undefined, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      assert.throws(() => total([{quantity: bad, unitCents: 100}]));
      assert.throws(() => total([{quantity: 1, unitCents: bad}]));
    }
    console.log(Object.hasOwn(invoice, 'default') ? 'faber:default-export' : 'faber:no-default-export');`;
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', code], { cwd: folder, windowsHide: true, encoding: 'utf8', timeout: 15000 });
  if (result.status !== 0) throw Error(result.error?.message ?? result.stderr ?? 'invoice failed');
  hasDefaultExport = result.stdout.trimEnd().split(/\r?\n/).at(-1) === 'faber:default-export';
});
check('a runnable regression test catches the zero-quantity charge', () => {
  // Nested proof checks must start a fresh runner, not inherit Node's test-child marker.
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  const parent = realpathSync.native(scratchRoot ?? dirname(resolve(pristine)));
  const implementation = `invoice-fixed-${randomUUID()}.mjs`;
  const reexport = `import { total as fixed } from './${implementation}';\nexport * from './${implementation}';\n`
    + (hasDefaultExport ? `export { default } from './${implementation}';\n` : '');
  const run = input => {
    const scratch = mkdtempSync(join(parent, '.invoice-proof-'));
    try {
      cpSync(folder, scratch, { recursive: true, filter: source => {
        if (['.git', 'node_modules'].includes(basename(source))) return false;
        if (lstatSync(source).isSymbolicLink()) throw Error('Regression inputs must be normal project files');
        return true;
      } });
      const entry = join(scratch, 'invoice.mjs');
      renameSync(entry, join(scratch, implementation));
      writeFileSync(entry, `${reexport}export function total(lines) { return fixed(${input}); }\n`);
      return spawnSync(process.execPath, ['--test', '--test-reporter=tap'], { cwd: scratch, env, windowsHide: true, encoding: 'utf8', timeout: 15000 });
    } finally {
      if (dirname(realpathSync.native(scratch)) !== parent) throw Error('Temporary regression folder changed location');
      rmSync(scratch, { recursive: true, force: true });
    }
  };
  const good = run('lines');
  if (good.status !== 0) throw Error(good.error?.message ?? good.stdout ?? 'regression test failed');
  if (!/^# tests [1-9]\d*\r?$/m.test(good.stdout)) throw Error('No regression tests ran');
  // Only the zero-quantity input changes. Restoring the whole original would also remove validation.
  const bad = run('Array.isArray(lines) ? lines.map(line => line?.quantity === 0 ? { ...line, quantity: 1 } : line) : lines');
  if (bad.status !== 1 || !/^# fail [1-9]\d*\r?$/m.test(bad.stdout)) {
    throw Error(bad.error?.message ?? 'The submitted regression test did not catch a charge for a zero quantity');
  }
});
console.log(JSON.stringify({ pass: checks.every(c => c.ok), checks }));
