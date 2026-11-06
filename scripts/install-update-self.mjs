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
  assert.ok(['tauri', 'clean', 'electron'].includes(scenario));
  assert.equal(process.platform, 'win32');
  assert.equal(process.env.GITHUB_ACTIONS, 'true');
  assert.equal(process.env.FABER_SMOKE_DISPOSABLE, '1');
  assert.match(process.env.FABER_PROOF_DESKTOP ?? '', /^faber-proof-[0-9a-f]{32}$/);
  const out = path.join(process.env.RUNNER_TEMP, 'install-update-evidence');
  mkdirSync(out, { recursive: true });
  const artifacts = path.join(process.env.RUNNER_TEMP, 'install-update-artifacts');
  const folder = path.join(process.env.RUNNER_TEMP, 'installed-faber');
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
    const args = scenario === 'tauri' ? ['--autostart'] : ['--force-renderer-accessibility'];
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
  let server;
  let hostsBefore;
  let thumbprint;
  const hosts = path.join(process.env.SystemRoot, 'System32/drivers/etc/hosts');
  try {
    assert.ok(!existsSync(data), 'The runner must have no pre-existing Faber data');
    assert.ok(!process.env.FABER_HIDDEN && !process.env.FABER_DATA_DIR, 'Both overrides disable the Tauri self-updater');
    mkdirSync(folder);
    const version = scenario === 'tauri' ? '0.3.17-preview.84' : scenario === 'electron' ? '0.3.18-preview.89' : '0.3.18-preview.90';
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
      assert.equal(JSON.parse(manifest).version, '0.3.18-preview.90');
      const installerUrl = new URL(JSON.parse(manifest).platforms['windows-x86_64'].url);
      assert.equal(installerUrl.hostname, 'github.com');
      const installerFile = path.join(artifacts, path.basename(installerUrl.pathname));
      let releaseDownload;
      const downloadAllowed = new Promise((resolve) => { releaseDownload = resolve; });
      if (scenario === 'tauri') releaseDownload();
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
      launch();
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
    if (scenario !== 'clean') {
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
      await until('The app installed Electron preview.90 itself', () => {
        const exe = readFileSync(path.join(folder, 'faber.exe'));
        if (scenario === 'tauri' && exe.equals(originalExe)) return false;
        // `status.version` is the channel crate's workspace version, not the packaged app release.
        const version = packageVersion(path.join(folder, 'resources/app.asar'));
        record('installed-package-version', version);
        if (version !== '0.3.18-preview.90') return false;
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
