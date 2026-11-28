export interface MenuItem {
  id: string;
  name: string;
  price: number;
}

export const menu: MenuItem[] = [
  { id: 'sourdough', name: 'Sourdough loaf', price: 6.5 },
  { id: 'seeded-rye', name: 'Seeded rye', price: 7 },
  { id: 'croissant', name: 'Butter croissant', price: 2.8 },
  { id: 'cinnamon-roll', name: 'Cinnamon roll', price: 3.4 },
  { id: 'lemon-tart', name: 'Lemon tart', price: 4.2 },
];
