import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

// The screen lives in ui/; Tauri loads the built files from dist/, or this dev server during `npm run dev`.
export default defineConfig({
  root: 'ui',
  base: './',
  plugins: [solid()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { outDir: '../dist', emptyOutDir: true, target: 'es2023' },
});
