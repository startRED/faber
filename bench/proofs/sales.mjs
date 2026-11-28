import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const [folder, pristine] = process.argv.slice(2), checks = [];
const check = (name, run) => {
  try { run(); checks.push({ name, ok: true }); }
  catch (error) { checks.push({ name, ok: false, detail: String(error.message).slice(0, 500) }); }
};
const summarize = path => spawnSync(process.execPath, [resolve(folder, 'summarize.mjs'), path], { cwd: folder, windowsHide: true, encoding: 'utf8', timeout: 15000 });
check('original CSV preserved and summarized', () => {
  const path = join(folder, 'sales.csv'), bytes = readFileSync(path);
  assert.deepEqual(bytes, readFileSync(join(pristine, 'sales.csv')));
  const result = summarize(path);
  assert.equal(result.status, 0);
  assert.deepEqual(JSON.parse(result.stdout), { Food: 1025, 'Office, supplies': 230, 'Client "A"': 100 });
  assert.deepEqual(readFileSync(path), bytes);
});
const temp = mkdtempSync(join(tmpdir(), 'faber-sales-proof-'));
try {
  check('fresh categories, refunds, rounding and empty export', () => {
    const path = join(temp, 'fresh.csv');
    for (const [text, expected] of [
      ['category,amount\nCafé,0.10\nCafé,0.20\n"Travel, rail",-4.05\n"Client ""B""",0.00\n', { Café: 30, 'Travel, rail': -405, 'Client "B"': 0 }],
      ['category,amount\n', {}],
      ['category,amount\nconstructor,1.00\n__proto__,2.00\nconstructor,-0.25\n', JSON.parse('{"constructor":75,"__proto__":200}')],
    ]) {
      writeFileSync(path, text);
      const result = summarize(path);
      assert.equal(result.status, 0);
      assert.deepEqual(JSON.parse(result.stdout), expected);
      assert.equal(readFileSync(path, 'utf8'), text);
    }
  });
  check('invalid headers, quoting and amounts fail', () => {
    const path = join(temp, 'invalid.csv');
    for (const text of ['name,price\nFood,1.00\n', 'category,amount,extra\nFood,1.00\n', 'category,amountBROKEN\nFood,1.00\n', 'category,amount\n"Food,1.00\n', 'category,amount\nFood,free\n', 'category,amount\nFood,1.005\n']) {
      writeFileSync(path, text);
      const result = summarize(path);
      assert.ok(Number.isInteger(result.status) && result.status !== 0, 'must exit nonzero, not hang');
      assert.equal(readFileSync(path, 'utf8'), text);
    }
  });
} finally { rmSync(temp, { recursive: true, force: true }); }
console.log(JSON.stringify({ pass: checks.every(c => c.ok), checks }));
