import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const original = readFileSync(new URL('./sales.csv', import.meta.url));
const directory = mkdtempSync(join(tmpdir(), 'summarize-'));
const file = join(directory, 'input.csv');
function run(csv) {
  writeFileSync(file, csv);
  return spawnSync(process.execPath, [fileURLToPath(new URL('./summarize.mjs', import.meta.url)), file], { encoding: 'utf8' });
}

try {
  for (const newline of ['\n', '\r\n']) {
    const result = run(['category,amount', 'food,1.10', 'food,-0.10', 'zero,0.00', '"a,b",2.34', '"say ""hi""",3.00', '__proto__,0.01', ''].join(newline));
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), Object.fromEntries([
      ['food', 100], ['zero', 0], ['a,b', 234], ['say "hi"', 300], ['__proto__', 1],
    ]));
  }
  for (const csv of ['category,amount', 'category,amount\n']) {
    const result = run(csv);
    assert.equal(result.status, 0);
    assert.equal(result.stdout.trim(), '{}');
  }
  for (const csv of ['', 'amount,category', 'category,amount\na,1.2', 'category,amount\na,+1.00', 'category,amount\n"a,1.00', 'category,amount\na"b,1.00', 'category,amount\n"a"x,1.00', 'category,amount\na,1.00,extra', 'category,amount\n\n']) {
    const result = run(csv);
    assert.notEqual(result.status, 0, csv);
    assert.equal(result.stdout, '');
  }
  const large = run('category,amount\nlarge,9007199254740993.01\nlarge,-0.01');
  assert.equal(large.status, 0, large.stderr);
  assert.equal(large.stdout.trim(), '{"large":900719925474099300}');
  assert.deepEqual(readFileSync(new URL('./sales.csv', import.meta.url)), original);
  console.log('All checks passed');
} finally {
  rmSync(directory, { recursive: true, force: true });
}
