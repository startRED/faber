import test from 'node:test';
import assert from 'node:assert/strict';
import { total } from './invoice.mjs';

test('totals integer cents, including zero quantities and empty invoices', () => {
  assert.equal(total([]), 0);
  assert.equal(total([{ quantity: 0, unitCents: 500 }]), 0);
  assert.equal(total([
    { quantity: 0, unitCents: 500 },
    { quantity: 2, unitCents: 125 },
    { quantity: 3, unitCents: 50 },
    { quantity: 4, unitCents: 0 },
  ]), 400);
  assert.equal(total([{ quantity: 1, unitCents: Number.MAX_SAFE_INTEGER }]), Number.MAX_SAFE_INTEGER);
});

test('rejects invalid quantity and unitCents, even on zero-value lines', () => {
  for (const field of ['quantity', 'unitCents']) {
    for (const value of [-1, 0.5, NaN, Infinity, -Infinity, Number.MAX_SAFE_INTEGER + 1,
      '1', null, undefined, true, 1n]) {
      assert.throws(() => total([{ quantity: 0, unitCents: 0, [field]: value }]), RangeError);
    }
  }
});

test('rejects unsafe products and accumulated totals', () => {
  assert.throws(() => total([{ quantity: 2, unitCents: Number.MAX_SAFE_INTEGER }]), RangeError);
  assert.throws(() => total([
    { quantity: 1, unitCents: Number.MAX_SAFE_INTEGER },
    { quantity: 1, unitCents: 1 },
  ]), RangeError);
});
