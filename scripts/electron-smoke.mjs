// Run only on a disposable Windows account: the installer still registers its own uninstall entry.
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32' || !process.argv[2]) throw new Error('Usage: node scripts/electron-smoke.mjs <Windows installer>');
if (process.env.FABER_SMOKE_DISPOSABLE !== '1') throw new Error('Set FABER_SMOKE_DISPOSABLE=1 only on a disposable Windows account');
const installer = path.resolve(process.argv[2]);
// electron-builder removes its previous installation before installing, so refuse a populated account.
const registry = path.join(process.env.SystemRoot, 'System32', 'reg.exe');
const guid = '27536615-8ccf-50e9-9bb7-91b15cd04c36';
for (const hive of ['HKCU', 'HKLM']) {
  for (const key of [`Software\\${guid}`, `Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${guid}`]) {
    const check = spawnSync(registry, ['query', `${hive}\\${key}`], { windowsHide: true, shell: false, encoding: 'utf8' });
    if (check.error || check.status === null || check.status === 0) throw new Error('An existing Electron Faber registration was found or could not be checked; use a disposable Windows account');
  }
}
const temp = mkdtempSync(path.join(os.tmpdir(), 'faber-smoke-'));
const folder = path.join(temp, 'Faber');
const env = { ...process.env, FABER_INSTALLER_SMOKE: '1', FABER_HIDDEN: '1', FABER_DATA_DIR: path.join(temp, 'data') };
const system = path.join(process.env.SystemRoot, 'System32');
let app;
let sidecarPid;
let installed = false;
function run(program, args) {
  const result = spawnSync(program, args, { env, windowsHide: true, shell: false, encoding: 'utf8', timeout: 120000 });
  if (result.error || result.status !== 0) throw result.error ?? new Error(`${program} failed (${result.status}): ${result.stderr}`);
  return result.stdout;
}
function requireFile(name) {
  if (!existsSync(path.join(folder, name))) throw new Error(`Missing ${name}`);
}
try {
  mkdirSync(folder);
  for (const name of ['faber.exe', 'uninstall.exe', 'faber-local.exe']) writeFileSync(path.join(folder, name), 'old Tauri file');
  run(installer, ['/S', '/UPDATE', `/D=${folder}`]);
  installed = true;
  for (const name of ['faber.exe', 'resources/app.asar', 'resources/faber-sidecar.exe', 'bin/faber.exe']) requireFile(name);
  for (const name of ['uninstall.exe', 'faber-local.exe']) if (existsSync(path.join(folder, name))) throw new Error(`Leftover ${name}`);
  if (readFileSync(path.join(folder, 'faber.exe')).subarray(0, 2).toString() !== 'MZ') throw new Error('Installed faber.exe is not a Windows executable');
  app = spawn(path.join(folder, 'faber.exe'), [], { env, windowsHide: true, shell: false, stdio: 'ignore' });
  app.on('error', () => {});
  let found = false;
  for (let i = 0; i < 30 && app.exitCode === null; i++) {
    const query = `Get-CimInstance Win32_Process -Filter \"ParentProcessId = ${app.pid}\" | Where-Object { $_.Name -eq 'faber-sidecar.exe' } | Select-Object -ExpandProperty ProcessId`;
    const child = run(path.join(system, 'WindowsPowerShell/v1.0/powershell.exe'), ['-NoProfile', '-NonInteractive', '-Command', query]).trim();
    if (child) { sidecarPid = Number(child); found = true; break; }
    await delay(1000);
  }
  if (!found) throw new Error('The sidecar did not start as a child');
  console.log('Installed files and sidecar child are present.');
} finally {
  if (app?.pid && app.exitCode === null) spawnSync(path.join(system, 'taskkill.exe'), ['/PID', String(app.pid), '/T', '/F'], { windowsHide: true, shell: false });
  if (sidecarPid) spawnSync(path.join(system, 'taskkill.exe'), ['/PID', String(sidecarPid), '/F'], { windowsHide: true, shell: false });
  if (installed) run(path.join(folder, 'Uninstall Faber.exe'), ['/S']);
  rmSync(temp, { recursive: true, force: true });
}
