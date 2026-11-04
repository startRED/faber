import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

test('draft import finishes before the application starts its host and screen', () => {
  const main = readFileSync(new URL('./main.ts', import.meta.url), 'utf8');
  const importing = main.indexOf('await migrateDrafts(dataDir)');
  assert.ok(importing > main.indexOf('serveScreen();'));
  assert.ok(importing < main.indexOf('host = Sidecar.start('));
});

async function prove(modes) {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'faber-profile-515b-'));
  const electron = createRequire(import.meta.url)('electron');
  try {
    if (modes.includes('failed-idle') || modes.includes('restart-dirty')) {
      const { build } = await import('vite');
      const { default: solid } = await import('vite-plugin-solid');
      const result = await build({ configFile: false, plugins: [solid()], logLevel: 'error', build: {
        write: false, minify: false, lib: { entry: fileURLToPath(new URL('./draft-screen.proof.mjs', import.meta.url)), name: 'draftScreen', formats: ['iife'] },
      } });
      writeFileSync(path.join(dir, 'screen.js'), result[0].output.find((file) => file.type === 'chunk').code);
    }
    for (const mode of modes) {
      const env = { ...process.env }; delete env.ELECTRON_RUN_AS_NODE;
      const child = spawn(electron, [fileURLToPath(new URL('./migration.proof.mjs', import.meta.url)), mode, dir], { env, windowsHide: true, shell: false, stdio: ['ignore', 'pipe', 'pipe'] });
      let output = '';
      child.stdout.on('data', (chunk) => { output += chunk; }); child.stderr.on('data', (chunk) => { output += chunk; });
      const code = await new Promise((resolve, reject) => { child.on('exit', resolve); child.on('error', reject); });
      assert.equal(code, 0, `${mode}: ${output}`);
      for (const line of output.split('\n').filter((line) => /^(PASS|COST)/.test(line))) console.log(line);
    }
  } finally { rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 }); }
}

test('Tauri localStorage and IndexedDB drafts survive the Electron origin change', { skip: process.platform !== 'win32' }, () => prove(['seed', 'import']));
test('attachment marker failure rolls back a partial field and retry restores its bytes', { skip: process.platform !== 'win32' }, () => prove(['seed', 'quota', 'import']));
test('retry after a partially completed migration never resurrects a consumed draft', { skip: process.platform !== 'win32' }, () => prove(['seed', 'partial', 'retry']));

test('failed import never restores a field used and consumed by the screen, including unreached fields', { skip: process.platform !== 'win32' }, () => prove(['seed', 'failed-use', 'retry-used']));

test('failed import followed by an untouched empty composer recovers the legacy draft after restart', { skip: process.platform !== 'win32' }, () => prove(['seed', 'failed-idle', 'import']));

test('engine recovery refreshes state while preserving a live composer whose draft write failed', () => prove(['restart-dirty']));
