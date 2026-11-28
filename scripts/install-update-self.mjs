// Installed-release proofs, only on disposable Windows CI accounts. No packaged application is patched.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:https';
import { createReadStream, existsSync, mkdirSync, readFileSync, readSync, writeFileSync, copyFileSync, openSync, closeSync, statSync, fstatSync } from 'node:fs';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

export const ID = '11111111-1111-4111-8111-111111111111';
export const TEXT = 'Keep this conversation through the update';
export const NEXT = 'Continued after the self-update';
export function previewVersion(tag) {
  assert.match(tag, /^v\d+\.\d+\.\d+-preview\.\d+$/);
  return tag.slice(1);
}
/** A published release tag, stable or preview: the target the reopen scenarios install. */
export function releaseVersion(tag) {
  assert.match(tag, /^v\d+\.\d+\.\d+(-preview\.\d+)?$/);
  return tag.slice(1);
}
/** The version the reopen scenarios build Faber at from source: just below `to`, so the signed `to` is an update. */
export function builtVersion(to) {
  return `${to.replace(/-.*$/, '')}-alpha.0`;
}

// The reopen scenarios build Faber from the checked-out code, so its `update/` runs the hand-off (a published release
// would test only its own, older helper). Window: the person's "Update ready, restart". Tray: the idle install of a Faber
// started with `--autostart`. First start: something else holds the single-instance lock when the helper starts Faber, so
// that start exits at once.
export const REOPEN = ['reopen-window', 'reopen-tray', 'first-start-exits'];
const TRAY = new Set(['tauri', 'reopen-tray', 'first-start-exits']);

/** The complete lines of `update.log` (`<unix seconds> <stage> <detail>`), oldest first; a cut or foreign line is skipped. */
export function parseUpdateLog(text) {
  return text.split('\n').slice(0, -1).flatMap((line) => {
    const found = /^(\d+) ([a-z][a-z-]*)(?: (.*))?$/.exec(line.replace(/\r$/, ''));
    return found ? [{ at: Number(found[1]), stage: found[2], detail: found[3] ?? '' }] : [];
  });
}

/** What the log says of the last finished install and the reopen after it. */
export function reopenEvidence(entries) {
  const from = entries.findLastIndex((entry) => entry.stage === 'installed');
  const after = from < 0 ? [] : entries.slice(from + 1);
  return {
    installed: from >= 0,
    retried: after.filter((entry) => entry.stage === 'reopen-retry').map((entry) => entry.detail),
    reopened: after.some((entry) => entry.stage === 'reopened' && /^in \d+ ms$/.test(entry.detail)),
    failed: after.find((entry) => entry.stage === 'reopen-failed')?.detail ?? null,
  };
}

/** Throws unless the evidence is what `scenario` must leave once Faber came back. */
export function expectReopen(scenario, evidence) {
  assert.ok(REOPEN.includes(scenario));
  assert.ok(evidence.installed, 'update.log must say the install finished');
  assert.equal(evidence.failed, null, 'update.log must not say the reopen failed');
  assert.ok(evidence.reopened, 'update.log must say Faber reopened after the install');
  if (scenario === 'first-start-exits') assert.ok(evidence.retried.length > 0, 'The start that exited at once must be retried');
}

// A resident helper for the first-start scenario, compiled before Faber starts so it is ready the moment the update hand-off begins.
// `Hold` waits until the old Faber has ended while the hand-off copy runs, then owns Chromium's single-instance window
// (Electron's class below, titled with the profile folder) and lets go of it once one start notifies it: that start found
// Faber "already running" and exited at once, as a start meeting a Faber not quite gone does.
export const WATCH = `
using System; using System.Collections.Generic; using System.Diagnostics; using System.IO;
using System.Runtime.InteropServices; using System.Threading;
public static class ProofWatch {
 public delegate IntPtr WndProc(IntPtr hwnd, uint msg, IntPtr w, IntPtr l);
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct WndClass {
  public uint size, style; public WndProc proc; public int clsExtra, wndExtra;
  public IntPtr instance, icon, cursor, background; public string menu, name; public IntPtr iconSmall; }
 [StructLayout(LayoutKind.Sequential)] public struct Msg { public IntPtr hwnd; public uint message; public IntPtr w, l; public uint time; public int x, y; }
 [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern ushort RegisterClassEx(ref WndClass c);
 [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateWindowEx(uint ex, string cls, string title, uint style, int x, int y, int w, int h, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr param);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr DefWindowProc(IntPtr h, uint m, IntPtr w, IntPtr l);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern bool DestroyWindow(IntPtr h);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetMessage(out Msg m, IntPtr h, uint min, uint max);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr DispatchMessage(ref Msg m);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern void PostQuitMessage(int code);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern UIntPtr SetTimer(IntPtr h, UIntPtr id, uint ms, IntPtr proc);
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] static extern IntPtr GetModuleHandle(string name);
 static WndProc keep; static bool notified;
 static bool Running(string name) { var all = Process.GetProcessesByName(name); foreach (var p in all) p.Dispose(); return all.Length > 0; }
 static List<int> Owned(string folder) {
  var found = new List<int>(); var cli = Path.Combine(folder, "bin") + "\\\\";
  foreach (var name in new[] { "faber", "faber-sidecar" }) foreach (var p in Process.GetProcessesByName(name)) {
   try { var file = p.MainModule.FileName; if (file.StartsWith(folder + "\\\\", StringComparison.OrdinalIgnoreCase) && !file.StartsWith(cli, StringComparison.OrdinalIgnoreCase)) found.Add(p.Id); } catch { }
   p.Dispose();
  }
  return found;
 }
 static bool WaitFor(Func<bool> check, int ms) {
  var end = DateTime.UtcNow.AddMilliseconds(ms);
  while (DateTime.UtcNow < end) { if (check()) return true; Thread.Sleep(10); }
  return false;
 }
 static void Watching() { Console.Out.WriteLine("watching"); Console.Out.Flush(); }
 public static string Hold(string folder, string title, int ms) {
  Watching();
  if (!WaitFor(() => Running("faber-update"), ms)) return "{\\"stage\\":\\"no-hand-off\\"}";
  if (!WaitFor(() => Owned(folder).Count == 0, ms)) return "{\\"stage\\":\\"old-faber-stayed\\"}";
  keep = (h, m, w, l) => {
   if (m == 0x004A) { notified = true; PostQuitMessage(0); return new IntPtr(1); }
   if (m == 0x0113) { PostQuitMessage(0); return IntPtr.Zero; }
   return DefWindowProc(h, m, w, l);
  };
  var c = new WndClass { size = (uint)Marshal.SizeOf(typeof(WndClass)), proc = keep, instance = GetModuleHandle(null), name = "Chrome_MessageWindow" };
  if (RegisterClassEx(ref c) == 0) return "{\\"stage\\":\\"no-class\\",\\"error\\":" + Marshal.GetLastWin32Error() + "}";
  var hwnd = CreateWindowEx(0, c.name, title, 0, 0, 0, 0, 0, new IntPtr(-3), IntPtr.Zero, c.instance, IntPtr.Zero);
  if (hwnd == IntPtr.Zero) return "{\\"stage\\":\\"no-window\\",\\"error\\":" + Marshal.GetLastWin32Error() + "}";
  var began = DateTime.UtcNow;
  SetTimer(hwnd, (UIntPtr)1, (uint)ms, IntPtr.Zero);
  Msg msg;
  while (GetMessage(out msg, IntPtr.Zero, 0, 0) > 0) DispatchMessage(ref msg);
  DestroyWindow(hwnd);
  return "{\\"stage\\":\\"" + (notified ? "notified" : "never-notified") + "\\",\\"heldMs\\":" + (int)(DateTime.UtcNow - began).TotalMilliseconds + "}";
 }
}
`;
export const SETTINGS = { language: 'en', previews: true, setup: 'done', tutorialDone: true, appearance: { base: 'sala' } };
export function sample() {
  return { id: ID, title: 'Self-update conversation', area: 'chat', createdAt: 1, updatedAt: 1,
    choice: { vendor: 'claude', model: 'sonnet', effort: 'medium' },
    messages: [{ id: 'seed', role: 'user', text: TEXT, at: 1 }] };
}
export function retained(value, settings) {
  assert.equal(value.id, ID);
  assert.ok(value.messages.some((m) => m.role === 'user' && m.text === TEXT), 'The app must read the original message');
  assert.equal(settings.language, 'en', 'The app must read the saved setting');
}

// Read only the bounded header and package metadata of our own installed ASAR; Windows drops prerelease version labels.
export function packageVersion(file) {
  const fd = openSync(file, 'r');
  try {
    const length = fstatSync(fd).size;
    const read = (at, size) => {
      assert.ok(Number.isSafeInteger(at) && at >= 0 && size > 0 && size <= 2 * 1024 * 1024 && at + size <= length);
      const bytes = Buffer.alloc(size);
      assert.equal(readSync(fd, bytes, 0, size, at), size);
      return bytes;
    };
    const prefix = read(0, 16);
    assert.equal(prefix.readUInt32LE(0), 4);
    const headerSize = prefix.readUInt32LE(4);
    const jsonSize = prefix.readUInt32LE(12);
    assert.ok(headerSize >= 8 + jsonSize && headerSize <= 2 * 1024 * 1024);
    const entry = JSON.parse(read(16, jsonSize)).files['package.json'];
    assert.ok(entry && !entry.unpacked && /^\d+$/.test(entry.offset) && Number.isSafeInteger(entry.size) && entry.size <= 65536);
    const pkg = JSON.parse(read(8 + headerSize + Number(entry.offset), entry.size));
    assert.equal(pkg.name, 'faber');
    assert.equal(typeof pkg.version, 'string');
    return pkg.version;
  } finally { closeSync(fd); }
}

async function main(scenario) {
  assert.ok(['tauri', 'clean', 'electron', ...REOPEN].includes(scenario));
  const reopen = REOPEN.includes(scenario);
  assert.equal(process.platform, 'win32');
  assert.equal(process.env.GITHUB_ACTIONS, 'true');
  assert.equal(process.env.FABER_SMOKE_DISPOSABLE, '1');
  assert.match(process.env.FABER_PROOF_DESKTOP ?? '', /^faber-proof-[0-9a-f]{32}$/);
  const out = path.join(process.env.RUNNER_TEMP, 'install-update-evidence');
  mkdirSync(out, { recursive: true });
  const artifacts = path.join(process.env.RUNNER_TEMP, 'install-update-artifacts');
  const folder = path.join(process.env.RUNNER_TEMP, 'installed-faber');
  const from = previewVersion(process.env.FABER_PROOF_FROM ?? 'v0.3.18-preview.89');
  const to = reopen ? releaseVersion(process.env.FABER_PROOF_REOPEN_TO ?? '') : previewVersion(process.env.FABER_PROOF_TO ?? 'v0.3.18-preview.90');
  process.env.FABER_PROOF_FOLDER = folder;
  const data = path.join(process.env.APPDATA, 'Faber');
  const ps = path.join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe');
  process.env.PSModulePath = path.join(path.dirname(ps), 'Modules');
  const observations = [];
  const record = (stage, detail) => {
    observations.push({ stage, detail, at: new Date().toISOString() });
    writeFileSync(path.join(out, 'observations.json'), JSON.stringify(observations, null, 2));
  };
  const run = (exe, args) => execFileSync(exe, args, { shell: false, windowsHide: true, encoding: 'utf8', timeout: 120000 });
  const powershell = (source) => run(ps, ['-NoProfile', '-NonInteractive', '-OutputFormat', 'Text', '-EncodedCommand', Buffer.from(`$ProgressPreference='SilentlyContinue'; Import-Module Microsoft.PowerShell.Security -ErrorAction Stop; ${source}`, 'utf16le').toString('base64')]);
  const ownedProcesses = () => JSON.parse(powershell(`$owned=@(Get-CimInstance Win32_Process -Filter "Name='faber.exe' OR Name='faber-sidecar.exe'" | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($env:FABER_PROOF_FOLDER,[StringComparison]::OrdinalIgnoreCase) } | Select-Object ProcessId,ParentProcessId,ExecutablePath); ConvertTo-Json -InputObject @($owned) -Compress`));
  let oldProcesses;
  const call = (cmd, args = {}) => JSON.parse(run(path.join(folder, 'bin/faber.exe'), ['--json', '--no-start', '--data', data, ...(cmd === 'status' ? ['status'] : ['call', cmd, JSON.stringify(args)])]));
  async function until(label, check, ms = 90000) {
    const end = Date.now() + ms;
    let failure;
    while (Date.now() < end) {
      try { const value = await check(); if (value) return value; } catch (error) { failure = error; }
      await delay(500);
    }
    throw new Error(`${label} timed out${failure ? `: ${failure.stdout?.toString() || failure.stderr?.toString() || failure.message}` : ''}`);
  }
  // UI Automation reads only HWNDs from our private desktop, with no debugging port or Electron fuse override.
  const uiaSource = `
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class ProofWindows {
 public delegate bool Visitor(IntPtr hwnd, IntPtr param);
 [DllImport("user32.dll")] public static extern IntPtr GetThreadDesktop(uint tid);
 [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
 [DllImport("user32.dll")] public static extern bool EnumDesktopWindows(IntPtr desktop, Visitor visit, IntPtr param);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
 public static List<IntPtr> All() {
  var found = new List<IntPtr>();
  EnumDesktopWindows(GetThreadDesktop(GetCurrentThreadId()), (h,p) => { found.Add(h); return true; }, IntPtr.Zero);
  return found;
 }
}
'@
$rows = @()
foreach ($hwnd in [ProofWindows]::All()) {
 [uint32]$owner = 0
 [void][ProofWindows]::GetWindowThreadProcessId($hwnd, [ref]$owner)
 try {
  $proc = Get-Process -Id $owner -ErrorAction Stop
  if ($proc.Path -ne $env:FABER_PROOF_EXE) { continue }
  $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
  $elements = $root.FindAll([Windows.Automation.TreeScope]::Subtree, [Windows.Automation.Condition]::TrueCondition)
  foreach ($element in $elements) {
   $name = $element.Current.Name
   if ($name) { $rows += @{ name=$name; type=$element.Current.ControlType.ProgrammaticName; pid=$owner } }
   if ($env:FABER_PROOF_CLICK -and $name.StartsWith($env:FABER_PROOF_CLICK) -and $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button) {
    $pattern = $element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
    $env:FABER_PROOF_CLICK = ''
   }
  }
 } catch { $rows += @{ error=$_.Exception.Message; pid=$owner } }
}
$json = ConvertTo-Json -InputObject @($rows) -Depth 4 -Compress
[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
`;
  const ui = (click) => {
    process.env.FABER_PROOF_EXE = path.join(folder, 'faber.exe');
    process.env.FABER_PROOF_CLICK = click ?? '';
    // A legacy console code page can turn curly quotes into unescaped JSON quotes; transport UTF-8 as ASCII.
    const raw = Buffer.from(powershell(uiaSource).trim(), 'base64').toString('utf8');
    writeFileSync(path.join(out, 'uia-raw.json'), raw);
    const rows = JSON.parse(raw);
    writeFileSync(path.join(out, 'ui-last.json'), JSON.stringify(rows, null, 2));
    return rows;
  };
  const launch = (stage = 'launched-baseline') => {
    // WebView2 cannot start on CI's private desktop (testkit/src/contain.rs); the real autostart path needs no webview.
    const args = TRAY.has(scenario) ? ['--autostart'] : ['--force-renderer-accessibility'];
    const stderr = openSync(path.join(out, 'app-stderr.log'), 'a');
    let app;
    try {
      app = spawn(path.join(folder, 'faber.exe'), args, { shell: false, windowsHide: true, stdio: ['ignore', 'ignore', stderr], env: process.env });
    } finally { closeSync(stderr); }
    app.on('error', (error) => record('launch-error', error.message));
    app.once('exit', (code, signal) => record('app-exited', { stage, code, signal }));
    app.unref();
    record(stage, { pid: app.pid });
    return app;
  };
  // Started without waiting. `ready` settles once it is compiled and watching (its first line, or its end), and the
  // update is served only then: a hand-off begun before it watched would be missed (run 46103155141). `done` is its result.
  const startWatch = (method) => {
    const source = `$ProgressPreference='SilentlyContinue'; Add-Type -TypeDefinition @'\n${WATCH}\n'@; [ProofWatch]::${method}`;
    const env = { ...process.env, FABER_PROOF_LOCK: path.join(data, 'webview') };
    const child = spawn(ps, ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(source, 'utf16le').toString('base64')], { shell: false, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'], env });
    let text = '';
    let watching;
    const ready = new Promise((resolve) => { watching = resolve; });
    child.stdout.on('data', (chunk) => { text += chunk; if (text.includes('watching')) watching(); });
    child.stderr.on('data', (chunk) => { text += chunk; });
    record('watch-started', { pid: child.pid, method });
    const done = new Promise((resolve) => child.once('close', (code) => {
      let result;
      try { result = JSON.parse(text.trim().split('\n').at(-1)); } catch { result = { stage: 'unreadable', code, text: text.slice(-2000) }; }
      record('watch-ended', result);
      watching();
      resolve(result);
    }));
    return { ready: ready.then(() => record('watch-ready', true)), done };
  };
  const mainProcesses = () => JSON.parse(powershell(`$exe=Join-Path $env:FABER_PROOF_FOLDER 'faber.exe'; $owned=@(Get-CimInstance Win32_Process -Filter "Name='faber.exe'" | Where-Object { $_.ExecutablePath -eq $exe -and $_.CommandLine -notmatch ' --type=' } | Select-Object ProcessId,CommandLine); ConvertTo-Json -InputObject @($owned) -Compress`));
  const updateLog = () => (existsSync(path.join(data, 'update.log')) ? readFileSync(path.join(data, 'update.log'), 'utf8') : '');
  // Faber, built from this code, updates itself to the signed `to` and must come back by itself, the way it was opened.
  async function reopenProof(watch) {
    if (scenario === 'reopen-window') {
      await until('The update is downloaded and waits for the button', () => call('update_status').phase === 'ready', 240000);
      await until('The person presses "Update ready, restart"', () => {
        const rows = ui();
        if (rows.some((row) => row.type === 'ControlType.Window' && /What.s new/.test(row.name ?? ''))) { ui('Continue'); return false; }
        if (!rows.some((row) => row.type === 'ControlType.Button' && row.name?.startsWith('Update ready, restart'))) return false;
        ui('Update ready, restart');
        record('old-app-update-restart-requested', true);
        return true;
      });
    }
    const asar = path.join(folder, 'resources/app.asar');
    await until(`Faber installed ${to} by itself`, () => existsSync(asar) && packageVersion(asar) === to, 240000);
    record('installed-package-version', to);
    const open = await until('Faber is open again after the update', () => {
      const running = mainProcesses();
      return running.length === 1 && call('status') && running;
    }, 150000).catch((error) => { record('faber-not-open', updateLog()); throw error; });
    record('reopened-processes', open);
    const tray = TRAY.has(scenario);
    assert.equal(open[0].CommandLine.includes('--autostart'), tray, tray ? 'The idle install reopens Faber in the tray' : 'The button reopens the window');
    if (!tray) {
      const rows = await until('The reopened window is on screen', () => { const rows = ui(); return rows.some((row) => row.type === 'ControlType.Window') && rows; });
      writeFileSync(path.join(out, 'window-after.json'), JSON.stringify(rows, null, 2));
    }
    const evidence = await until('update.log says how the reopen ended', () => {
      const found = reopenEvidence(parseUpdateLog(updateLog()));
      return (found.reopened || found.failed) && found;
    }, 150000).catch((error) => { record('update-log', updateLog()); throw error; });
    record('reopen-evidence', evidence);
    expectReopen(scenario, evidence);
    if (watch) {
      const watched = await watch.done;
      if (scenario === 'first-start-exits') assert.equal(watched.stage, 'notified', 'A start must have met the held single-instance lock');
    }
    assert.ok(observations.some((o) => o.stage === 'updater-request' && o.detail.endsWith('_x64-setup.exe')), 'Faber fetched the update itself');
  }
  let server;
  let hostsBefore;
  let thumbprint;
  const hosts = path.join(process.env.SystemRoot, 'System32/drivers/etc/hosts');
  try {
    assert.ok(!existsSync(data), 'The runner must have no pre-existing Faber data');
    assert.ok(!process.env.FABER_HIDDEN && !process.env.FABER_DATA_DIR, 'Both overrides disable the Tauri self-updater');
    mkdirSync(folder);
    const version = scenario === 'tauri' ? '0.3.17-preview.84' : scenario === 'electron' ? from : reopen ? builtVersion(to) : to;
    // The only installer we invoke is the baseline. The target of either update is fetched and installed by Faber itself.
    run(path.join(artifacts, `Faber_${version}_x64-setup.exe`), ['/S', `/D=${folder}`]);
    record('baseline-installed', version);
    const originalExe = readFileSync(path.join(folder, 'faber.exe'));
    if (scenario !== 'clean') {
      mkdirSync(path.join(data, 'conversations'), { recursive: true });
      writeFileSync(path.join(data, 'conversations', `${ID}.json`), JSON.stringify(sample()));
      writeFileSync(path.join(data, 'settings.json'), JSON.stringify(SETTINGS));
    }

    // A recorded provider lets the continuity assertion run without credentials or subscription costs.
    // This exercises Faber's real CLI discovery/parser/session resumption, not an external provider's availability.
    if (scenario === 'electron') {
      const fake = path.join(process.env.RUNNER_TEMP, 'recorded-provider');
      mkdirSync(fake);
      writeFileSync(path.join(fake, 'claude.cmd'), '@"%~dp0\\node.exe" "%~dp0\\replay.cjs" %*\r\n');
      copyFileSync(process.execPath, path.join(fake, 'node.exe'));
      writeFileSync(path.join(fake, 'replay.cjs'), `
const fs = require('node:fs');
if (process.argv.includes('--version')) { console.log('2.1.0 (Claude Code)'); process.exit(0); }
if (process.argv.includes('--help')) { console.log('--input-format --output-format --include-partial-messages --permission-prompt-tool --effort'); process.exit(0); }
if (process.argv.includes('auth')) { console.log(JSON.stringify({loggedIn:true,authMethod:'claude.ai',subscriptionType:'max'})); process.exit(0); }
if (!process.argv.includes('-p')) { console.log('{}'); process.exit(0); }
const mark = ${JSON.stringify(path.join(out, 'provider-started'))};
const again = fs.existsSync(mark);
const emit = value => console.log(JSON.stringify(value));
emit({type:'system',subtype:'init',session_id:'22222222-2222-4222-8222-222222222222',tools:[],model:'claude-sonnet-4-6'});
require('node:readline').createInterface({input:process.stdin}).on('line', line => {
 const message = JSON.parse(line);
 if (message.type === 'control_request' && message.request?.subtype === 'interrupt') process.exit(0);
 if (message.type !== 'user') return;
 fs.writeFileSync(again ? ${JSON.stringify(path.join(out, 'provider-resumed'))} : mark, JSON.stringify({pid:process.pid,at:Date.now(),args:process.argv.slice(2)}));
 if (!again) return;
 emit({type:'stream_event',event:{type:'content_block_delta',delta:{type:'text_delta',text:${JSON.stringify(NEXT)}}}});
 emit({type:'assistant',message:{id:'continued',role:'assistant',content:[{type:'text',text:${JSON.stringify(NEXT)}}],model:'claude-sonnet-4-6',usage:{input_tokens:1,output_tokens:1}}});
 emit({type:'result',subtype:'success',is_error:false,result:${JSON.stringify(NEXT)},session_id:'22222222-2222-4222-8222-222222222222',total_cost_usd:0,usage:{input_tokens:1,output_tokens:1}});
}).on('close', () => process.exit(0));
setInterval(()=>{},1000);
`);
      process.env.PATH = `${fake};${process.env.PATH}`;
    }

    if (scenario !== 'clean') {
      // Pin the unsigned feed in this VM only; use the original manifest and signed installer, without disabling TLS
      // or signature verification. The disposable hosted runner is an administrator; machine roots need no user dialog.
      const certDir = path.join(process.env.RUNNER_TEMP, 'proof-tls');
      mkdirSync(certDir);
      process.env.FABER_PROOF_CERT = path.join(certDir, 'github.pfx');
      thumbprint = powershell(`$ErrorActionPreference='Stop'; (New-SelfSignedCertificate -DnsName github.com -CertStoreLocation Cert:\\LocalMachine\\My).Thumbprint`).trim();
      assert.match(thumbprint, /^[0-9A-F]{40}$/);
      record('certificate-created', thumbprint);
      powershell(`$ErrorActionPreference='Stop'; $cert=Get-Item -LiteralPath 'Cert:\\LocalMachine\\My\\${thumbprint}'; Export-PfxCertificate -Cert $cert -FilePath $env:FABER_PROOF_CERT -Password (ConvertTo-SecureString 'disposable-proof' -AsPlainText -Force) | Out-Null; $root=New-Object Security.Cryptography.X509Certificates.X509Store('Root','LocalMachine'); $root.Open('ReadWrite'); $root.Add($cert); $root.Close()`);
      record('certificate-trusted', true);
      const manifest = readFileSync(path.join(artifacts, 'latest.json'));
      assert.equal(JSON.parse(manifest).version, to);
      const installerUrl = new URL(JSON.parse(manifest).platforms['windows-x86_64'].url);
      assert.equal(installerUrl.hostname, 'github.com');
      const installerFile = path.join(artifacts, path.basename(installerUrl.pathname));
      let releaseDownload;
      const downloadAllowed = new Promise((resolve) => { releaseDownload = resolve; });
      // The scenario with a watcher serves the update once it watches (`startWatch`).
      if (!['electron', 'first-start-exits'].includes(scenario)) releaseDownload();
      server = createServer({ pfx: readFileSync(process.env.FABER_PROOF_CERT), passphrase: 'disposable-proof' }, async (req, res) => {
        record('updater-request', req.url);
        if (req.url === '/startRED/faber/releases/download/preview-channel/latest.json' || req.url === '/startRED/faber/releases/latest/download/latest.json') {
          res.writeHead(200, { 'Content-Type': 'application/json', 'Content-Length': manifest.length });
          res.end(manifest);
        } else if (req.url === installerUrl.pathname) {
          await downloadAllowed;
          res.writeHead(200, { 'Content-Type': 'application/octet-stream' });
          createReadStream(installerFile).pipe(res);
        } else { res.writeHead(404); res.end(); }
      });
      await new Promise((resolve, reject) => { server.once('error', reject); server.listen(443, '127.0.0.1', resolve); });
      hostsBefore = readFileSync(hosts);
      writeFileSync(hosts, Buffer.concat([hostsBefore, Buffer.from('\r\n127.0.0.1 github.com # disposable Faber proof\r\n')]));
      powershell('Clear-DnsClientCache');
      const watch = scenario === 'first-start-exits' ? startWatch('Hold($env:FABER_PROOF_FOLDER, $env:FABER_PROOF_LOCK, 300000)') : null;
      watch?.ready.then(releaseDownload);
      launch();
      if (reopen) await reopenProof(watch);
      if (scenario === 'electron') {
        await until('The old app data API', () => call('conversation', { id: ID }));
        call('send', { req: { conversationId: ID, text: 'Continue this conversation after updating', area: 'chat', images: [], files: [], choice: sample().choice } });
        await until('The pre-update CLI session', () => existsSync(path.join(out, 'provider-started')));
        const before = call('conversation', { id: ID });
        assert.equal(before.state, 'working');
        assert.ok(!before.messages.some((m) => m.text.includes(NEXT)), 'Continuation must not appear before the update');
        record('turn-running-before-update', before);
        releaseDownload();
        await until('Downloaded update is waiting for the running conversation', () => call('update_status').phase === 'ready');
        const opened = await until('Conversation displayed before update', () => {
          const rows = ui();
          if (rows.some((row) => row.type === 'ControlType.Window' && /What.s new/.test(row.name ?? ''))) { ui('Continue'); return false; }
          if (rows.some((row) => row.name?.includes(TEXT))) return rows;
          if (rows.some((row) => row.type === 'ControlType.Button' && row.name?.startsWith('Self-update conversation'))) ui('Self-update conversation');
          return false;
        });
        writeFileSync(path.join(out, 'conversation-open-before.json'), JSON.stringify(opened, null, 2));
        record('conversation-open-before', true);
        const atRestart = call('conversation', { id: ID });
        assert.equal(atRestart.state, 'working');
        assert.ok(!atRestart.messages.some((m) => m.text.includes(NEXT)), 'No continuation may predate the restart');
        record('conversation-at-restart', atRestart);
        oldProcesses = ownedProcesses();
        assert.ok(oldProcesses.length > 0);
        record('old-app-processes', oldProcesses);
        record('old-app-update-restart-requested', true);
        ui('Update ready, restart');
      }
    } else {
      launch();
      const rows = await until('First-run subscription/sign-in entry screen', () => {
        const rows = ui();
        return rows.some((r) => /Connect your subscriptions|Conecte suas assinaturas/.test(r.name)) && rows;
      });
      writeFileSync(path.join(out, 'first-run-screen.json'), JSON.stringify(rows, null, 2));
      record('clean-first-run-screen', rows);
      record('limit', 'Observed the subscription connection screen; no real provider authentication or sent message.');
    }
    // The reopen scenarios proved their part in `reopenProof`; keeping data across versions is the electron scenario's.
    if (scenario !== 'clean' && !reopen) {
      if (scenario === 'tauri') {
        await until('The old updater installed the Electron payload', () =>
          ['resources/app.asar', 'resources/faber-sidecar.exe', 'bin/faber.exe'].every((name) => existsSync(path.join(folder, name))) && !readFileSync(path.join(folder, 'faber.exe')).equals(originalExe), 240000);
        record('old-app-installed-electron-payload', true);
        // The old plugin's /R is a Tauri-installer convention. Start the already self-installed app only to read its data.
        record('limit', 'The already self-installed app is started explicitly for data reads; automatic relaunch from Tauri preview.84 is not asserted.');
        launch('launched-self-installed-app-for-data-read');
        await until('The new app created its migration backup', () => existsSync(path.join(data, 'backup-before-electron.done')));
        const backup = readFileSync(path.join(data, 'backup-before-electron.done'), 'utf8').trim();
        assert.match(backup, /^backup-before-electron-[0-9a-f-]+$/i);
        retained(JSON.parse(readFileSync(path.join(data, backup, 'conversations', `${ID}.json`))), JSON.parse(readFileSync(path.join(data, backup, 'settings.json'))));
        assert.ok(readFileSync(path.join(folder, 'previous/faber-tauri.exe')).equals(originalExe));
        record('backup-verified', backup);
      }
      await until(`The app installed Electron ${to} itself`, () => {
        const exe = readFileSync(path.join(folder, 'faber.exe'));
        if (scenario === 'tauri' && exe.equals(originalExe)) return false;
        // `status.version` is the channel crate's workspace version, not the packaged app release.
        const version = packageVersion(path.join(folder, 'resources/app.asar'));
        record('installed-package-version', version);
        if (version !== to) return false;
        return call('status');
      }, 240000);
      record('self-update-version', call('status'));
      const conversation = call('conversation', { id: ID });
      const settings = call('settings');
      retained(conversation, settings);
      record('running-app-read-conversation-and-setting', { conversation, settings });
      assert.ok(observations.some((o) => o.stage === 'updater-request' && o.detail.endsWith('/latest.json')));
      assert.ok(observations.some((o) => o.stage === 'updater-request' && o.detail.endsWith('_x64-setup.exe')));
      if (scenario === 'electron') {
        const currentProcesses = ownedProcesses();
        record('new-app-processes', currentProcesses);
        assert.ok(oldProcesses.every((old) => !currentProcesses.some((current) => current.ProcessId === old.ProcessId)), 'All old app processes must exit before the new host answers');
        record('old-app-processes-ended', true);
        await until('A new provider session receives the resumed user input', () => existsSync(path.join(out, 'provider-resumed')));
        const first = JSON.parse(readFileSync(path.join(out, 'provider-started')));
        const resumed = JSON.parse(readFileSync(path.join(out, 'provider-resumed')));
        assert.notEqual(first.pid, resumed.pid);
        assert.ok(resumed.at > first.at);
        assert.ok(resumed.at >= Date.parse(observations.find((o) => o.stage === 'old-app-update-restart-requested').at));
        record('resumed-provider-process', { first, resumed });
        const continued = await until('The same conversation continues after the update', () => {
          const value = call('conversation', { id: ID });
          return value.messages.some((m) => m.role === 'assistant' && m.text.includes(NEXT)) && value;
        });
        retained(continued, call('settings'));
        record('conversation-continued-after', continued);
        const reopened = await until('The same conversation and continued reply are visible after restart', () => {
          const rows = ui();
          if (rows.some((row) => row.type === 'ControlType.Window' && /What.s new/.test(row.name ?? ''))) { ui('Continue'); return false; }
          return rows.some((row) => row.name?.includes(TEXT)) && rows.some((row) => row.name?.includes(NEXT)) && rows;
        });
        writeFileSync(path.join(out, 'conversation-open-after.json'), JSON.stringify(reopened, null, 2));
        record('conversation-open-after', true);
      }
    }
    writeFileSync(path.join(out, 'result.json'), JSON.stringify({ scenario, pass: true, observations }, null, 2));
  } catch (error) {
    record('failure', [error.stack, error.stdout?.toString(), error.stderr?.toString()].filter(Boolean).join('\n'));
    try {
      process.env.FABER_PROOF_FOLDER = folder;
      const diagnostic = powershell(`$owned=@(Get-CimInstance Win32_Process -Filter "Name='faber.exe' OR Name='faber-sidecar.exe'" | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($env:FABER_PROOF_FOLDER,[StringComparison]::OrdinalIgnoreCase) } | Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine); $pipes=@([IO.Directory]::GetFiles('\\\\.\\pipe\\') | Where-Object { $_ -like '*faber-*' }); @{ processes=$owned; pipes=$pipes; roaming=[Environment]::GetFolderPath('ApplicationData'); environmentRoaming=$env:APPDATA } | ConvertTo-Json -Depth 4 -Compress`);
      writeFileSync(path.join(out, 'owned-processes.json'), diagnostic);
      record('data-directory', data);
      if (existsSync(path.join(folder, 'resources/app.asar'))) record('installed-package-version-at-failure', packageVersion(path.join(folder, 'resources/app.asar')));
      const secretFile = path.join(data, 'channel/secret');
      record('channel-file-metadata', { exists: existsSync(secretFile), bytes: existsSync(secretFile) ? statSync(secretFile).size : null });
      try {
        const fd = openSync(secretFile, 'r');
        closeSync(fd);
        record('channel-file-readable-without-reading', true);
      } catch (accessError) { record('channel-file-readable-without-reading', accessError.code); }
      const connectivity = powershell(`$results=@(); foreach ($name in [IO.Directory]::GetFiles('\\\\.\\pipe\\') | Where-Object { $_ -like '*faber-*' }) { $client=[IO.Pipes.NamedPipeClientStream]::new('.', [IO.Path]::GetFileName($name), [IO.Pipes.PipeDirection]::InOut); try { $client.Connect(1000); $results+=@{pipe=$name;connected=$true} } catch { $results+=@{pipe=$name;connected=$false;error=$_.Exception.Message;hresult=$_.Exception.HResult} } finally { $client.Dispose() } }; ConvertTo-Json -InputObject @($results) -Compress`);
      writeFileSync(path.join(out, 'pipe-connectivity.json'), connectivity);
    } catch (diagnosticError) { record('diagnostic-failed', diagnosticError.message); }
    writeFileSync(path.join(out, 'result.json'), JSON.stringify({ scenario, pass: false, error: error.message, observations }, null, 2));
    process.exitCode = 1;
  } finally {
    for (const name of ['update.log', 'startup.log']) if (existsSync(path.join(data, name))) copyFileSync(path.join(data, name), path.join(out, name));
    if (hostsBefore) writeFileSync(hosts, hostsBefore);
    if (thumbprint && /^[0-9A-F]{40}$/.test(thumbprint)) powershell(`Remove-Item -LiteralPath 'Cert:\\LocalMachine\\Root\\${thumbprint}' -ErrorAction SilentlyContinue; Remove-Item -LiteralPath 'Cert:\\LocalMachine\\My\\${thumbprint}' -ErrorAction SilentlyContinue`);
    if (server) { server.closeAllConnections(); server.close(); }
    // Leave app/browser processes alone. GitHub disposes the VM after evidence upload.
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main(process.argv[2]);
