import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Only in `npm run dev`: hot reload needs an inline script and a websocket, which the shipped policy forbids.
const devPolicy = {
  name: 'faber-dev-policy',
  apply: 'serve' as const,
  transformIndexHtml: (html: string) => html.replace("default-src 'self';", "default-src 'self'; script-src 'self' 'unsafe-inline'; connect-src 'self' ws://localhost:*;"),
};

export default defineConfig({
  root: 'src/renderer',
  base: './',
  plugins: [react(), devPolicy],
  build: { outDir: '../../dist/renderer', emptyOutDir: true },
});
