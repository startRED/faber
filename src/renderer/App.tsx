const page = {
  minHeight: '100vh',
  margin: 0,
  display: 'grid',
  placeItems: 'center',
  background: '#111216',
  color: '#e6e8ef',
  fontFamily: 'system-ui, sans-serif',
} as const;

export function App({ version }: { version: string }) {
  return (
    <main style={page}>
      <div style={{ textAlign: 'center' }}>
        <h1 style={{ fontSize: 48, margin: 0 }}>Faber</h1>
        <p style={{ color: '#9ba2b3' }}>One app for everything AI can do · v{version}</p>
        <p style={{ color: '#9ba2b3', fontSize: 13 }}>Early development</p>
      </div>
    </main>
  );
}
