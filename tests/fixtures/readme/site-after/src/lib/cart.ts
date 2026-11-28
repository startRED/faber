import type { MenuItem } from '../data/menu';

export interface BasketItem extends MenuItem {
  quantity: number;
}

export function addItem(basket: BasketItem[], item: MenuItem): BasketItem[] {
  const found = basket.find((line) => line.id === item.id);
  if (!found) return [...basket, { ...item, quantity: 1 }];
  return basket.map((line) => (line.id === item.id ? { ...line, quantity: line.quantity + 1 } : line));
}

export function removeItem(basket: BasketItem[], id: string): BasketItem[] {
  return basket
    .map((line) => (line.id === id ? { ...line, quantity: line.quantity - 1 } : line))
    .filter((line) => line.quantity > 0);
}

const toCents = (line: BasketItem) => Math.round(line.price * 100) * line.quantity;

/** Adds in cents, so three croissants at 2.80 come to 8.40 and never 8.399999. */
export function orderTotal(basket: BasketItem[]): number {
  return basket.reduce((sum, line) => sum + toCents(line), 0) / 100;
}
