import { defineConfig } from 'vite';
import { lstatSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import solid from 'vite-plugin-solid';
import { woff2OnlyPlugin } from './scripts/woff2-only.mjs';

// The screen lives in ui/; Tauri loads the built files from dist/, or this dev server during `npm run dev`.
export default defineConfig({
  root: 'ui',
  base: './',
  plugins: [solid(), {
    name: 'test-build-output',
    apply: 'build',
    configResolved(config) {
      if (config.mode !== 'test') return;
      const test = resolve(config.root, '../dist-test');
      if (resolve(config.root, config.build.outDir) !== test) {
        throw new Error('The test build must write to dist-test/. Pass --outDir ../dist-test.');
      }
      const production = resolve(config.root, '../dist');
      if ([test, production].some((dir) => lstatSync(dir, { throwIfNoEntry: false })?.isSymbolicLink())) {
        throw new Error('The test build needs separate dist-test/ and dist/ folders, without linked output folders.');
      }
    },
  }, {
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
