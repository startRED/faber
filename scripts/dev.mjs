// `npm run dev`: the app runs from the source. Screen changes show at once (Vite hot reload); changes to the engine,
// main process or preload are rebuilt and the window reopens by itself. Closing the window ends everything.
import { spawn, spawnSync } from 'node:child_process';
import { watch } from 'node:fs';
import { createRequire } from 'node:module';
import { createServer } from 'vite';

const electronBin = createRequire(import.meta.url)('electron');
const buildMain = () =>
  spawnSync(process.execPath, ['node_modules/typescript/bin/tsc', '-p', 'tsconfig.main.json'], { stdio: 'inherit', maxBuffer: 16 * 1024 * 1024 }).status === 0;

const server = await createServer({ configFile: 'vite.config.ts', server: { port: 5199, strictPort: false } });
await server.listen();
const url = server.resolvedUrls.local[0];

let app;
let restarting = false;
function start() {
  app = spawn(electronBin, ['.'], { stdio: 'inherit', env: { ...process.env, FABER_DEV_URL: url } });
  app.on('exit', () => {
    if (restarting) {
      restarting = false;
      start();
    } else {
      void server.close().then(() => process.exit(0));
    }
  });
}

// Stopping the dev script (Ctrl+C, closing its console) closes the app too.
for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(signal, () => (app.kill(), process.exit(0)));

buildMain();
start();
console.log(`faber dev: screen served from ${url}`);

let timer;
watch('src', { recursive: true }, (_event, file) => {
  if (!file || !/^(main|engine|preload)[\\/]/.test(file)) return;
  clearTimeout(timer);
  timer = setTimeout(() => {
    if (!buildMain()) return; // a broken build keeps the running app
    console.log(`faber dev: ${file} changed, reopening the window`);
    restarting = true;
    app.kill();
  }, 300);
});
