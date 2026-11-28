import { describe, expect, it } from 'vitest';
import { pickupTimes } from '../src/components/PickupTimePicker';
import { menu } from '../src/data/menu';
import { addItem, orderTotal, removeItem, type BasketItem } from '../src/lib/cart';

const croissant = menu.find((item) => item.id === 'croissant')!;
const sourdough = menu.find((item) => item.id === 'sourdough')!;

describe('the basket', () => {
  it('starts a new line for a first item', () => {
    expect(addItem([], croissant)).toEqual([{ ...croissant, quantity: 1 }]);
  });

  it('counts the same item up instead of repeating it', () => {
    const basket = addItem(addItem([], croissant), croissant);
    expect(basket).toHaveLength(1);
    expect(basket[0]!.quantity).toBe(2);
  });

  it('drops a line when its last item is removed', () => {
    expect(removeItem(addItem([], sourdough), sourdough.id)).toEqual([]);
  });
});

describe('the total', () => {
  it('is zero for an empty basket', () => {
    expect(orderTotal([])).toBe(0);
  });

  it('adds three croissants to exactly 8.40', () => {
    const basket: BasketItem[] = [{ ...croissant, quantity: 3 }];
    expect(orderTotal(basket)).toBe(8.4);
  });

  it('adds different items together', () => {
    const basket = addItem(addItem([], croissant), sourdough);
    expect(orderTotal(basket)).toBe(9.3);
  });
});

describe('the pickup time', () => {
  it('offers every half hour before the shop opens', () => {
    expect(pickupTimes(new Date(2026, 4, 12, 6, 0))).toHaveLength(16);
  });

  it('never offers a time that has passed', () => {
    expect(pickupTimes(new Date(2026, 4, 12, 13, 10))[0]).toBe('13:30');
  });

  it('offers nothing after closing', () => {
    expect(pickupTimes(new Date(2026, 4, 12, 15, 0))).toEqual([]);
  });
});
