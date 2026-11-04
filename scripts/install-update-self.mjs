// Installed-release proofs, only on disposable Windows CI accounts. No packaged application is patched.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:https';
import { createReadStream, existsSync, mkdirSync, readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

export const ID = '11111111-1111-4111-8111-111111111111';
export const TEXT = 'Keep this conversation through the update';
export const NEXT = 'Continued after the self-update';
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
  const observations = [];
  const record = (stage, detail) => {
    observations.push({ stage, detail, at: new Date().toISOString() });
    writeFileSync(path.join(out, 'observations.json'), JSON.stringify(observations, null, 2));
  };
  const run = (exe, args) => execFileSync(exe, args, { shell: false, windowsHide: true, encoding: 'utf8', timeout: 120000 });
  const powershell = (source) => run(ps, ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(source, 'utf16le').toString('base64')]);
  const call = (cmd, args = {}) => JSON.parse(run(path.join(folder, 'bin/faber.exe'), ['--json', '--no-start', ...(cmd === 'status' ? ['status'] : ['call', cmd, JSON.stringify(args)])]));
  async function until(label, check, ms = 90000) {
    const end = Date.now() + ms;
    let failure;
    while (Date.now() < end) {
      try { const value = await check(); if (value) return value; } catch (error) { failure = error; }
      await delay(500);
    }
    throw new Error(`${label} timed out${failure ? `: ${failure.message}` : ''}`);
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
   if ($env:FABER_PROOF_CLICK -and $name -eq $env:FABER_PROOF_CLICK) {
    $pattern = $element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
    $env:FABER_PROOF_CLICK = ''
   }
  }
 } catch { $rows += @{ error=$_.Exception.Message; pid=$owner } }
}
ConvertTo-Json -InputObject @($rows) -Depth 4 -Compress
`;
  const ui = (click) => {
    process.env.FABER_PROOF_EXE = path.join(folder, 'faber.exe');
    process.env.FABER_PROOF_CLICK = click ?? '';
    return JSON.parse(powershell(uiaSource));
  };
  const launch = () => {
    const app = spawn(path.join(folder, 'faber.exe'), ['--force-renderer-accessibility'], {
      shell: false, windowsHide: true, stdio: 'ignore', env: process.env,
    });
    app.on('error', (error) => record('launch-error', error.message));
    app.unref();
    record('launched-baseline', { pid: app.pid });
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
      writeFileSync(path.join(data, 'settings.json'), JSON.stringify({ language: 'en', previews: true, setup: 'done', tutorialDone: true, appearance: { base: 'dark-room' } }));
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
 if (JSON.parse(line).type !== 'user') return;
 fs.writeFileSync(again ? ${JSON.stringify(path.join(out, 'provider-resumed'))} : mark, JSON.stringify({pid:process.pid,at:Date.now(),args:process.argv.slice(2)}));
 if (!again) return;
 emit({type:'assistant',message:{id:'continued',role:'assistant',content:[{type:'text',text:${JSON.stringify(NEXT)}}],model:'claude-sonnet-4-6',usage:{input_tokens:1,output_tokens:1}}});
 emit({type:'result',subtype:'success',is_error:false,result:${JSON.stringify(NEXT)},session_id:'22222222-2222-4222-8222-222222222222',total_cost_usd:0,usage:{input_tokens:1,output_tokens:1}});
});
setInterval(()=>{},1000);
`);
      process.env.PATH = `${fake};${process.env.PATH}`;
    }

    if (scenario !== 'clean') {
      // Pin the unsigned feed in this VM only; use the original manifest and signed installer, without disabling TLS
      // or signature verification. Both published clients trust the Windows certificate store.
      const certDir = path.join(process.env.RUNNER_TEMP, 'proof-tls');
      mkdirSync(certDir);
      process.env.FABER_PROOF_CERT = path.join(certDir, 'github.pfx');
      thumbprint = powershell(`$ErrorActionPreference='Stop'; $cert=New-SelfSignedCertificate -DnsName github.com -CertStoreLocation Cert:\\CurrentUser\\My; Export-PfxCertificate -Cert $cert -FilePath $env:FABER_PROOF_CERT -Password (ConvertTo-SecureString 'disposable-proof' -AsPlainText -Force) | Out-Null; $root=New-Object Security.Cryptography.X509Certificates.X509Store('Root','CurrentUser'); $root.Open('ReadWrite'); $root.Add($cert); $root.Close(); $cert.Thumbprint`).trim();
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
        call('show_conversation', { id: ID });
        const opened = await until('Conversation displayed before update', () => {
          const rows = ui();
          return rows.some((row) => row.name.includes(TEXT)) && rows;
        });
        writeFileSync(path.join(out, 'conversation-open-before.json'), JSON.stringify(opened, null, 2));
        record('conversation-open-before', true);
        record('old-app-update-restart-requested', true);
        try { call('update_restart'); } catch (error) { record('restart-channel-result', error.message); }
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
      await until('The app installed Electron preview.90 itself', () => {
        const exe = readFileSync(path.join(folder, 'faber.exe'));
        if (exe.equals(originalExe)) return false;
        return call('status').version === '0.3.18-preview.90';
      }, 240000);
      record('self-update-version', call('status'));
      const conversation = call('conversation', { id: ID });
      const settings = call('settings');
      retained(conversation, settings);
      record('running-app-read-conversation-and-setting', { conversation, settings });
      assert.ok(observations.some((o) => o.stage === 'updater-request' && o.detail.endsWith('/latest.json')));
      assert.ok(observations.some((o) => o.stage === 'updater-request' && o.detail.endsWith('_x64-setup.exe')));
      if (scenario === 'tauri') {
        const backup = readFileSync(path.join(data, 'backup-before-electron.done'), 'utf8').trim();
        assert.match(backup, /^backup-before-electron-[0-9a-f-]+$/i);
        retained(JSON.parse(readFileSync(path.join(data, backup, 'conversations', `${ID}.json`))), JSON.parse(readFileSync(path.join(data, backup, 'settings.json'))));
        assert.ok(readFileSync(path.join(folder, 'previous/faber-tauri.exe')).equals(originalExe));
        record('backup-verified', backup);
      } else {
        await until('A new provider session receives the resumed user input', () => existsSync(path.join(out, 'provider-resumed')));
        const first = JSON.parse(readFileSync(path.join(out, 'provider-started')));
        const resumed = JSON.parse(readFileSync(path.join(out, 'provider-resumed')));
        assert.notEqual(first.pid, resumed.pid);
        assert.ok(resumed.at > first.at);
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
    record('failure', error.stack);
    writeFileSync(path.join(out, 'result.json'), JSON.stringify({ scenario, pass: false, error: error.message, observations }, null, 2));
    process.exitCode = 1;
  } finally {
    for (const name of ['update.log', 'startup.log']) if (existsSync(path.join(data, name))) copyFileSync(path.join(data, name), path.join(out, name));
    if (hostsBefore) writeFileSync(hosts, hostsBefore);
    if (thumbprint && /^[0-9A-F]{40}$/.test(thumbprint)) powershell(`Remove-Item -LiteralPath 'Cert:\\CurrentUser\\Root\\${thumbprint}'; Remove-Item -LiteralPath 'Cert:\\CurrentUser\\My\\${thumbprint}'`);
    if (server) { server.closeAllConnections(); server.close(); }
    // Leave app/browser processes alone. GitHub disposes the VM after evidence upload.
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main(process.argv[2]);
