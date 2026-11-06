export const total = lines => lines.reduce((sum, { quantity, unitCents }) => {
  if (!Number.isSafeInteger(quantity) || quantity < 0 ||
      !Number.isSafeInteger(unitCents) || unitCents < 0) {
    throw new RangeError('quantity and unitCents must be nonnegative safe integers');
  }
  const next = sum + quantity * unitCents;
  if (!Number.isSafeInteger(next)) {
    throw new RangeError('Invoice total exceeds the safe integer range');
  }
  return next;
}, 0);
