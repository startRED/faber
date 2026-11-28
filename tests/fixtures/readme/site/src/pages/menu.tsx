import { menu } from '../data/menu';

export function Menu() {
  return (
    <main>
      <h1>Today's menu</h1>
      <ul className="menu">
        {menu.map((item) => (
          <li key={item.id}>
            <span>{item.name}</span>
            <span>${item.price.toFixed(2)}</span>
          </li>
        ))}
      </ul>
    </main>
  );
}
