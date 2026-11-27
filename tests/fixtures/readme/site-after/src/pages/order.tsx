import { useState } from 'react';
import { PickupTimePicker } from '../components/PickupTimePicker';
import { menu } from '../data/menu';
import { addItem, orderTotal, removeItem, type BasketItem } from '../lib/cart';

export function Order() {
  const [basket, setBasket] = useState<BasketItem[]>([]);
  const [pickup, setPickup] = useState('');
  const [orderNumber, setOrderNumber] = useState<string>();

  if (orderNumber) {
    return (
      <main>
        <h1>Thank you!</h1>
        <p>Order {orderNumber} will be ready at {pickup}. Pay at the counter when you collect it.</p>
      </main>
    );
  }

  return (
    <main>
      <h1>Order for pickup</h1>
      <ul className="menu">
        {menu.map((item) => (
          <li key={item.id}>
            <span>{item.name}</span>
            <span>${item.price.toFixed(2)}</span>
            <button type="button" onClick={() => setBasket(addItem(basket, item))}>Add</button>
          </li>
        ))}
      </ul>

      <h2>Your basket</h2>
      {basket.length === 0 && <p>Nothing here yet. Add something from the menu.</p>}
      <ul className="basket">
        {basket.map((line) => (
          <li key={line.id}>
            <span>{line.quantity} × {line.name}</span>
            <button type="button" onClick={() => setBasket(removeItem(basket, line.id))}>Remove</button>
          </li>
        ))}
      </ul>
      <p className="total">Total ${orderTotal(basket).toFixed(2)}</p>

      <PickupTimePicker value={pickup} onChange={setPickup} />
      <button
        type="button"
        disabled={basket.length === 0 || !pickup}
        onClick={() => setOrderNumber(`B-${Date.now().toString().slice(-5)}`)}
      >
        Place order
      </button>
    </main>
  );
}
