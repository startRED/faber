import { defineConfig } from 'vite';
import { readFileSync } from 'node:fs';
import solid from 'vite-plugin-solid';
import { woff2OnlyPlugin } from './scripts/woff2-only.mjs';

// The screen lives in ui/; Tauri loads the built files from dist/, or this dev server during `npm run dev`.
export default defineConfig({
  root: 'ui',
  base: './',
  plugins: [solid(), {
    name: 'visual-runtime',
    transformIndexHtml: {
      order: 'post',
      handler: (html) => ({
        html: html.replace(/(<script\b[^>]*\btype=)["']module["']/g, '$1"application/faber-module"')
          .replace(/<link\b[^>]*\brel=["']modulepreload["'][^>]*>/g, ''),
        tags: [{ tag: 'script', children: readFileSync(new URL('./ui/visual-runtime.js', import.meta.url), 'utf8').replace(/\r\n?/g, '\n'), injectTo: 'body' as const }],
      }),
    },
  }],
  // The webview reads woff2: the woff copies Fontsource lists as a fallback are left out of the build.
  css: { postcss: { plugins: [woff2OnlyPlugin()] } },
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { outDir: '../dist', emptyOutDir: true, target: 'es2023' },
});
