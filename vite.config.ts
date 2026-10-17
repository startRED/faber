import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
import { woff2OnlyPlugin } from './scripts/woff2-only.mjs';

// The screen lives in ui/; Tauri loads the built files from dist/, or this dev server during `npm run dev`.
export default defineConfig({
  root: 'ui',
  base: './',
  plugins: [solid()],
  // The webview reads woff2: the woff copies Fontsource lists as a fallback are left out of the build.
  css: { postcss: { plugins: [woff2OnlyPlugin()] } },
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { outDir: '../dist', emptyOutDir: true, target: 'es2023' },
});
