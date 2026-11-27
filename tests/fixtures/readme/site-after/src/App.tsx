import { Route, Routes } from 'react-router-dom';
import { Contact } from './pages/contact';
import { Home } from './pages/home';
import { Menu } from './pages/menu';
import { Order } from './pages/order';

export function App() {
  return (
    <Routes>
      <Route path="/" element={<Home />} />
      <Route path="/menu" element={<Menu />} />
      <Route path="/order" element={<Order />} />
      <Route path="/contact" element={<Contact />} />
    </Routes>
  );
}
