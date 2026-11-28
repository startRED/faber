import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  plugins: [react({ jsxImportSource: 'react' })],
  // This cache lives below Faber's Solid tsconfig; keep its React runtime separate.
  esbuild: { jsx: 'automatic', jsxImportSource: 'react' },
  publicDir: '../demo-public',
  server: { host: '127.0.0.1', strictPort: true, fs: { allow: ['..'] } },
  resolve: { dedupe: ['react', 'react-dom'], alias: { './preview': fileURLToPath(new URL('../examples/faber/frame.tsx', import.meta.url)) } },
  build: { target: 'es2022', outDir: '../dist-faber', emptyOutDir: true, rollupOptions: { input: { viewer: fileURLToPath(new URL('./faber.html', import.meta.url)), frame: fileURLToPath(new URL('./index.html', import.meta.url)) } } },
});
