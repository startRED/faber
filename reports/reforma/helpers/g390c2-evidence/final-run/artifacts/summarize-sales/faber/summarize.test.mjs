import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const sales = new URL('./sales.csv', import.meta.url);
const original = readFileSync(sales);
const directory = mkdtempSync(join(tmpdir(), 'summarize-'));
const input = join(directory, 'input.csv');
const run = (csv) => {
  writeFileSync(input, csv);
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('./summarize.mjs', import.meta.url)), input], { encoding: 'utf8' });
  if (result.error) throw result.error;
  return result;
};
try {
  for (const newline of ['\n', '\r\n']) {
    const result = run(['category,amount', 'food,1.20', 'food,-0.20', 'zero,0.00', '"a,b",2.34', '"a""b",-1.01', '__proto__,0.01', '"quoted","3.00"', ''].join(newline));
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), JSON.parse('{"food":100,"zero":0,"a,b":234,"a\\"b":-101,"__proto__":1,"quoted":300}'));
  }
  for (const csv of ['category,amount', 'category,amount\n', 'category,amount\r\n']) {
    const result = run(csv);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), '{}');
  }
  for (const csv of ['', 'amount,category\na,1.00', 'category,amount \n', ...[
    'a,1', 'a,1.0', 'a,1.000', 'a,+1.00', 'a, 1.00', 'a,NaN', 'a,1.00,extra',
    '"a,1.00', 'a"b,1.00', '"a"x,1.00', '"a"b",1.00', 'a,"1.00', 'a,1.\r00', '',
  ].map(row => `category,amount\n${row}\n`)]) {
    const result = run(csv);
    assert.notEqual(result.status, 0, JSON.stringify(csv));
    assert.equal(result.stdout, '');
  }
  const large = run('category,amount\nbig,9007199254740993.01\nbig,-0.01');
  assert.equal(large.status, 0, large.stderr);
  assert.equal(large.stdout.trim(), '{"big":900719925474099300}');
  assert.deepEqual(readFileSync(sales), original);
  console.log('All checks passed; sales.csv is byte-for-byte unchanged.');
} finally {
  rmSync(directory, { recursive: true, force: true });
}
