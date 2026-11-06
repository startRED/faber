import test from 'node:test';
import assert from 'node:assert/strict';
import { total } from './invoice.mjs';

test('totals integer cents, including zero quantities and empty invoices', () => {
  assert.equal(total([]), 0);
  assert.equal(total([{ quantity: 0, unitCents: 125 }]), 0);
  assert.equal(total([
    { quantity: 0, unitCents: 999 },
    { quantity: 2, unitCents: 125 },
    { quantity: 3, unitCents: 10 },
    { quantity: 5, unitCents: 0 },
  ]), 280);
  assert.equal(total([{ quantity: 1, unitCents: Number.MAX_SAFE_INTEGER }]), Number.MAX_SAFE_INTEGER);
});

test('rejects invalid quantities and prices even when the other field is zero', () => {
  const invalid = [-1, 0.5, NaN, Infinity, -Infinity,
    Number.MAX_SAFE_INTEGER + 1, '2', null, undefined, true, 1n];
  for (const field of ['quantity', 'unitCents']) {
    for (const value of invalid) {
      for (const other of [0, 1]) {
        assert.throws(() => total([
          { quantity: other, unitCents: other, [field]: value },
        ]), RangeError);
      }
    }
    const line = { quantity: 1, unitCents: 1 };
    delete line[field];
    assert.throws(() => total([line]), RangeError);
  }
});

test('rejects totals that cannot be represented exactly in integer cents', () => {
  assert.throws(() => total([
    { quantity: 2, unitCents: Number.MAX_SAFE_INTEGER },
  ]), RangeError);
  assert.throws(() => total([
    { quantity: 1, unitCents: Number.MAX_SAFE_INTEGER },
    { quantity: 1, unitCents: 1 },
  ]), RangeError);
});
