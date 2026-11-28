import type { MenuItem } from '../data/menu';

export interface BasketItem extends MenuItem {
  quantity: number;
}

export function addItem(basket: BasketItem[], item: MenuItem): BasketItem[] {
  const found = basket.find((line) => line.id === item.id);
  if (!found) return [...basket, { ...item, quantity: 1 }];
  return basket.map((line) => (line.id === item.id ? { ...line, quantity: line.quantity + 1 } : line));
}

export function basketTotal(basket: BasketItem[]): number {
  return basket.reduce((sum, line) => sum + line.price * line.quantity, 0);
}
