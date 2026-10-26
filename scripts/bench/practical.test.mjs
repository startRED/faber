import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../../', import.meta.url));
const valid = {
  invoice: ['invoice.mjs', `export function total(lines) {
    return lines.reduce((sum, { quantity, unitCents }) => {
      if (![quantity, unitCents].every(n => Number.isSafeInteger(n) && n >= 0)) throw Error('invalid');
      return sum + quantity * unitCents;
    }, 0);
  }`],
  settings: ['migrate.mjs', `import { readFileSync, writeFileSync } from 'node:fs';
    const path = process.argv[2], value = JSON.parse(readFileSync(path, 'utf8'));
    if (value.version === 2) process.exit(0);
    if (value.version !== 1 || typeof value.theme !== 'string') process.exit(1);
    const { theme, ...rest } = value;
    writeFileSync(path, JSON.stringify({ ...rest, version: 2, appearance: { theme } }));`],
  sales: ['summarize.mjs', `import { readFileSync } from 'node:fs';
    const text = readFileSync(process.argv[2], 'utf8');
    if (text.split(/\\r?\\n/)[0] !== 'category,amount') process.exit(1);
    const rows = text.trim().split(/\\r?\\n/).slice(1), totals = Object.create(null);
    for (const row of rows) {
      const match = row.match(/^(?:"((?:[^"]|"")*)"|([^",]+)),(-?\\d+\\.\\d{2})$/);
      if (!match) process.exit(1);
      const key = (match[1] ?? match[2]).replaceAll('""', '"');
      const amount = Math.round(Number(match[3]) * 100);
      totals[key] = (totals[key] ?? 0) + amount;
    }
    console.log(JSON.stringify(totals));`],
};

function workspace(name, run) {
  assert.ok(existsSync(join(root, 'bench/fixtures', `${name}.mjs`)), `${name}: benchmark fixture is provided`);
  const base = mkdtempSync(join(tmpdir(), 'faber-practical-'));
  try {
    const pristine = join(base, 'pristine'), folder = join(base, 'result');
    mkdirSync(pristine); mkdirSync(folder);
    for (const dest of [pristine, folder]) execFileSync(process.execPath, [join(root, 'bench/fixtures', `${name}.mjs`), dest], { windowsHide: true });
    const answer = join(base, 'answer.md'); writeFileSync(answer, 'Completed.');
    const proof = () => {
      const result = spawnSync(process.execPath, [join(root, 'bench/proofs', `${name}.mjs`), folder, pristine, answer], { encoding: 'utf8', windowsHide: true, timeout: 15000 });
      assert.equal(result.status, 0, result.stderr);
      const parsed = JSON.parse(result.stdout);
      assert.ok(parsed.checks.length > 0);
      return parsed.pass;
    };
    run({ folder, pristine, proof });
  } finally { rmSync(base, { recursive: true, force: true }); }
}

function complete(name, folder) {
  const [file, code] = valid[name]; writeFileSync(join(folder, file), code);
  if (name === 'invoice') writeFileSync(join(folder, 'invoice.test.mjs'), `import assert from 'node:assert/strict'; import { total } from './invoice.mjs'; assert.equal(total([{quantity: 0, unitCents: 900}]), 0);`);
  if (name === 'settings') execFileSync(process.execPath, [join(folder, file), join(folder, 'settings.json')], { windowsHide: true });
}

for (const name of Object.keys(valid)) {
  test(`${name}: unfinished work fails, a complete result passes, source loss fails`, () => workspace(name, ({ folder, proof }) => {
    assert.equal(proof(), false, 'the starting folder is not a completed task');
    complete(name, folder);
    assert.equal(proof(), true, 'an independently written solution meets the contract');
    const source = { invoice: 'README.md', settings: 'notes.txt', sales: 'sales.csv' }[name];
    writeFileSync(join(folder, source), 'lost data');
    assert.equal(proof(), false, 'a working result cannot hide lost source data');
  }));
}

test('invoice proof rejects the zero-quantity bug despite a passing ordinary example', () => workspace('invoice', ({ folder, proof }) => {
  complete('invoice', folder);
  assert.equal(proof(), true);
  writeFileSync(join(folder, 'invoice.mjs'), 'export const total = lines => lines.reduce((sum, x) => sum + (x.quantity || 1) * x.unitCents, 0);');
  assert.equal(proof(), false);
}));

test('settings proof rejects a migrator that discards unknown preferences', () => workspace('settings', ({ folder, proof }) => {
  complete('settings', folder);
  assert.equal(proof(), true);
  const code = valid.settings[1].replace('...rest, version: 2', 'version: 2');
  writeFileSync(join(folder, 'migrate.mjs'), code);
  assert.equal(proof(), false);
}));

test('settings proof rejects presence validation that accepts non-string themes', () => workspace('settings', ({ folder, proof }) => {
  complete('settings', folder);
  assert.equal(proof(), true);
  writeFileSync(join(folder, 'migrate.mjs'), valid.settings[1].replace("typeof value.theme !== 'string'", 'value.theme == null'));
  assert.equal(proof(), false, 'numbers, booleans, arrays and objects are not string themes');
}));

test('settings proof requires empty strings to remain valid themes', () => workspace('settings', ({ folder, proof }) => {
  complete('settings', folder);
  assert.equal(proof(), true);
  writeFileSync(join(folder, 'migrate.mjs'), valid.settings[1].replace("typeof value.theme !== 'string'", "typeof value.theme !== 'string' || value.theme.length === 0"));
  assert.equal(proof(), false, 'the contract requires a string without a nonempty restriction');
}));

test('sales proof rejects a hardcoded fixture answer on fresh input', () => workspace('sales', ({ folder, proof }) => {
  complete('sales', folder);
  assert.equal(proof(), true);
  writeFileSync(join(folder, 'summarize.mjs'), `console.log(JSON.stringify({ Food: 1025, 'Office, supplies': 230, 'Client "A"': 100 }));`);
  assert.equal(proof(), false);
}));

test('invoice accepts a CommonJS regression test supported by node --test', () => workspace('invoice', ({ folder, proof }) => {
  complete('invoice', folder);
  rmSync(join(folder, 'invoice.test.mjs'));
  writeFileSync(join(folder, 'invoice.test.cjs'), `const assert = require('node:assert/strict'); import('./invoice.mjs').then(({total}) => assert.equal(total([{quantity: 0, unitCents: 100}]), 0));`);
  assert.equal(proof(), true);
}));

test('invoice uses native discovery for regression tests in a test directory', () => workspace('invoice', ({ folder, proof }) => {
  complete('invoice', folder);
  rmSync(join(folder, 'invoice.test.mjs'));
  mkdirSync(join(folder, 'test'));
  writeFileSync(join(folder, 'test', 'regression.cjs'), `const assert = require('node:assert/strict'); import('../invoice.mjs').then(({total}) => assert.equal(total([{quantity: 0, unitCents: 100}]), 0));`);
  assert.equal(proof(), true);
}));

test('sales proof rejects prefix-only header validation', () => workspace('sales', ({ folder, proof }) => {
  complete('sales', folder);
  assert.equal(proof(), true);
  writeFileSync(join(folder, 'summarize.mjs'), valid.sales[1].replace("text.split(/\\r?\\n/)[0] !== 'category,amount'", "!text.startsWith('category,amount')"));
  assert.equal(proof(), false);
}));

test('sales proof rejects collisions with inherited object properties', () => workspace('sales', ({ folder, proof }) => {
  complete('sales', folder);
  assert.equal(proof(), true);
  writeFileSync(join(folder, 'summarize.mjs'), valid.sales[1].replace('Object.create(null)', '{}'));
  assert.equal(proof(), false);
}));

test('new requests name the real fixture and proof entry points', () => {
  for (const [number, id, name] of [[6, 'fix-invoice', 'invoice'], [7, 'migrate-settings', 'settings'], [8, 'summarize-sales', 'sales']]) {
    const path = join(root, 'bench/requests', `${number}-${id}.md`);
    assert.ok(existsSync(path), `${id}: benchmark request is provided`);
    const text = readFileSync(path, 'utf8');
    for (const header of [`id: ${id}`, 'kind: project', `fixture: ${name}`, `proof: ${name}`]) assert.ok(text.includes(header), header);
  }
});
