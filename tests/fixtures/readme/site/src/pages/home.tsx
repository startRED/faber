import { Link } from 'react-router-dom';

export function Home() {
  return (
    <main>
      <h1>Fresh bread, every morning</h1>
      <p>Baked from 5 a.m. on Mill Street. Come early for the cinnamon rolls.</p>
      <Link to="/menu">See today's menu</Link>
    </main>
  );
}
