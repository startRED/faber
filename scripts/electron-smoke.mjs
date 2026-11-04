// Run only on a disposable Windows account: the installer still registers its own uninstall entry.
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32' || !process.argv[2] || !process.argv[3]) throw new Error('Usage: node scripts/electron-smoke.mjs <Electron installer> <old Tauri installer>');
if (process.env.FABER_SMOKE_DISPOSABLE !== '1') throw new Error('Set FABER_SMOKE_DISPOSABLE=1 only on a disposable Windows account');
const installer = path.resolve(process.argv[2]);
const oldInstaller = path.resolve(process.argv[3]);
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
// Remove the temp folder, retrying only lock errors with a bounded backoff (about 60 s); anything else throws.
async function removeTemp() {
  for (let attempt = 0; ; attempt++) {
    try { return rmSync(temp, { recursive: true, force: true }); } catch (error) {
      if (!['EPERM', 'EBUSY', 'ENOTEMPTY', 'EACCES'].includes(error?.code) || attempt >= 30) throw error;
      await delay(Math.min(500 * 2 ** attempt, 3000));
    }
  }
}
try {
  mkdirSync(folder);
  run(oldInstaller, ['/S', `/D=${folder}`]);
  requireFile('faber.exe');
  const oldExe = readFileSync(path.join(folder, 'faber.exe'));
  mkdirSync(env.FABER_DATA_DIR);
  const conversation = 'conversations/11111111-1111-4111-8111-111111111111.json';
  const sample = {
    [conversation]: JSON.stringify({ id: '11111111-1111-4111-8111-111111111111', title: 'Migration test', area: 'chat', createdAt: 1, updatedAt: 1, messages: [{ id: 'message', role: 'user', text: 'Keep this conversation', at: 1 }] }),
    'settings.json': JSON.stringify({ language: 'en' }),
  };
  mkdirSync(path.join(env.FABER_DATA_DIR, 'conversations'));
  for (const [name, contents] of Object.entries(sample)) writeFileSync(path.join(env.FABER_DATA_DIR, name), contents);
  run(installer, ['/S', '/UPDATE', `/D=${folder}`]);
  installed = true;
  for (const name of ['faber.exe', 'resources/app.asar', 'resources/faber-sidecar.exe', 'bin/faber.exe']) requireFile(name);
  if (!readFileSync(path.join(folder, 'previous/faber-tauri.exe')).equals(oldExe)) throw new Error('The previous Tauri executable was not preserved');
  for (const name of ['uninstall.exe', 'faber-local.exe']) if (existsSync(path.join(folder, name))) throw new Error(`Leftover ${name}`);
  for (const [name, contents] of Object.entries(sample)) if (readFileSync(path.join(env.FABER_DATA_DIR, name), 'utf8') !== contents) throw new Error(`Existing ${name} was changed by the installer`);
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
  const marker = path.join(env.FABER_DATA_DIR, 'backup-before-electron.done');
  for (let i = 0; i < 30 && !existsSync(marker) && app.exitCode === null; i++) await delay(1000);
  if (!existsSync(marker)) throw new Error('The data backup was not completed');
  const backupName = readFileSync(marker, 'utf8').trim();
  if (!/^backup-before-electron-[0-9a-f-]+$/i.test(backupName)) throw new Error('Invalid backup folder');
  for (const [name, contents] of Object.entries(sample)) {
    if (readFileSync(path.join(env.FABER_DATA_DIR, backupName, name), 'utf8') !== contents) throw new Error(`Backup of ${name} differs`);
    const current = JSON.parse(readFileSync(path.join(env.FABER_DATA_DIR, name), 'utf8'));
    const expected = JSON.parse(contents);
    if (name === conversation ? current.messages?.[0]?.text !== expected.messages[0].text : current.language !== expected.language) throw new Error(`Existing ${name} was not retained after startup`);
  }
  console.log('Installed files and sidecar child are present.');
} finally {
  if (app?.pid && app.exitCode === null) spawnSync(path.join(system, 'taskkill.exe'), ['/PID', String(app.pid), '/T', '/F'], { windowsHide: true, shell: false });
  if (sidecarPid) spawnSync(path.join(system, 'taskkill.exe'), ['/PID', String(sidecarPid), '/F'], { windowsHide: true, shell: false });
  if (installed) {
    // The NSIS uninstaller copies itself aside and returns at once, so files stay locked for a while.
    try { run(path.join(folder, 'Uninstall Faber.exe'), ['/S']); } finally { await removeTemp(); }
  } else await removeTemp();
}
