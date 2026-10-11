import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { isAbsolute, join, relative, resolve } from 'node:path';
import { chromium, type Browser, type Locator, type Page } from 'playwright-core';
import { en } from '../../ui/locales/en.ts';
import { replay, type Entry } from '../support/replay.ts';

const root = resolve(import.meta.dirname, '../..');
const binary = join(root, 'target/e2e/debug/faber.exe');
const fake = join(root, 'tests/support/fake-cli.ts');
const recordings = join(root, 'tests/fixtures/setup');
const skip = process.platform !== 'win32' && 'requires the real Windows WebView2 app';
const delay = (ms: number) => new Promise<void>((done) => setTimeout(done, ms));

// Poll a condition, then fail with an assertion carrying the criterion, rather than a Playwright
// timeout. Every caller still exercises the real UI, executable, and engine.
async function eventually(check: () => boolean | Promise<boolean>, message: string | (() => string), ms = 15_000) {
  const deadline = Date.now() + ms;
  do {
    if (await check()) return;
    await delay(100);
  } while (Date.now() < deadline);
  assert.fail(typeof message === 'function' ? message() : message);
}

type ProcessRow = { pid: number; parent: number; name: string; born: string; bytes: number };
const powershell = join(process.env.SystemRoot ?? 'C:\\Windows', 'System32/WindowsPowerShell/v1.0/powershell.exe');
function processes(): ProcessRow[] {
  const script = '$memory = @{}; Get-CimInstance Win32_PerfRawData_PerfProc_Process | ForEach-Object { ' +
    '$memory[[int]$_.IDProcess] = $_.WorkingSetPrivate }; ' +
    '@(Get-CimInstance Win32_Process | ForEach-Object { [pscustomobject]@{ pid = [int]$_.ProcessId; ' +
    'parent = [int]$_.ParentProcessId; name = $_.Name; born = [string]$_.CreationDate.Ticks; bytes = $memory[[int]$_.ProcessId] } }) | ConvertTo-Json -Compress';
  return JSON.parse(execFileSync(powershell, ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8', windowsHide: true }));
}
// A child is born after its parent. Windows reuses process ids, so a process whose parent id now belongs to a younger
// process (csrss.exe and winlogon.exe outlive the smss.exe that started them) is not part of the tree. `born` is the
// creation time in ticks, all the same width, so text order is time order.
function descendants(rows: ProcessRow[], pid: number): ProcessRow[] {
  const born = new Map([[pid, rows.find((row) => row.pid === pid)?.born ?? '']]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const row of rows) {
      const parent = born.get(row.parent);
      if (parent !== undefined && !born.has(row.pid) && row.born >= parent) {
        born.set(row.pid, row.born);
        changed = true;
      }
    }
  }
  return rows.filter((row) => born.has(row.pid));
}
function userPath(): string {
  return execFileSync(powershell, ['-NoProfile', '-NonInteractive', '-Command',
    '[Environment]::GetEnvironmentVariable("Path", "User")'], { encoding: 'utf8', windowsHide: true }).trim();
}
function contained(base: string, path: string): boolean {
  const rel = relative(base, resolve(path));
  return !isAbsolute(rel) && rel !== '..' && !rel.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`);
}
function snapshot(dir: string, exclude?: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (exclude && (path === exclude || path.startsWith(exclude + '/') || path.startsWith(exclude + '\\'))) continue;
    if (entry.isDirectory()) Object.assign(result, snapshot(path, exclude));
    else result[path] = createHash('sha256').update(readFileSync(path)).digest('hex');
  }
  return result;
}
type Command = { pid: number; ppid: number; vendor: string; cwd: string; args: string[]; targets?: string[] };
type Run = { page: Page; browser: Browser; app: ChildProcess; pid: number; seen: ProcessRow[] };

async function fixture(t: TestContext, connected = false) {
  assert.ok(existsSync(binary), 'build the real app with npm run test:e2e before running setup acceptance proofs');
  // CI temp can be an 8.3 short path (RUNNER~1) while processes report the long one; compare long paths.
  const root = realpathSync.native(mkdtempSync(join(tmpdir(), 'faber-setup-e2e-')));
  // Also clean up failures during fixture construction, before dispose is available.
  t.after(() => rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 }));
  const observed = join(root, 'observed');
  const enclosing = join(observed, 'enclosing');
  const boundary = join(enclosing, 'boundary');
  const temp = join(boundary, 'scenario');
  const sandbox = join(temp, 'sandbox');
  const data = join(sandbox, 'data');
  const state = join(sandbox, 'fake-state');
  const profile = join(sandbox, 'profile');
  const canary = join(temp, 'outside-sandbox');
  for (const path of [data, state, profile, canary, join(sandbox, 'path'), join(sandbox, 'tmp'),
    join(profile, 'AppData/Local'), join(profile, 'AppData/Roaming')]) mkdirSync(path, { recursive: true });
  writeFileSync(join(canary, 'untouched.txt'), 'No installation or login may write here.');
  const before = snapshot(canary);
  const originalEnv = { ...process.env };
  const originalPath = userPath();
  writeFileSync(join(data, 'settings.json'), JSON.stringify({ language: 'en' }));
  if (connected) writeFileSync(join(state, 'claude.signed-in'), 'true');

  // Named like the official release binary, served zipped like the official release asset.
  const downloaded = join(sandbox, 'release', 'codex-x86_64-pc-windows-msvc.exe');
  mkdirSync(join(sandbox, 'release'));
  const compiler = join(process.env.SystemRoot ?? 'C:\\Windows', 'Microsoft.NET/Framework64/v4.0.30319/csc.exe');
  assert.ok(existsSync(compiler), 'the fixture needs the Windows C# compiler for its executable download');
  const compile = spawnSync(compiler, ['/nologo', '/target:exe', `/out:${downloaded}`, join(recordings, 'launcher.cs')],
    { shell: false, windowsHide: true, encoding: 'utf8' });
  assert.equal(compile.status, 0, `test launcher compilation: ${compile.stdout}${compile.stderr}`);
  const payload = readFileSync(downloaded);
  const zipped = join(sandbox, 'codex-x86_64-pc-windows-msvc.exe.zip');
  execFileSync(powershell, ['-NoProfile', '-NonInteractive', '-Command',
    `Compress-Archive -LiteralPath '${downloaded}' -DestinationPath '${zipped}'`], { windowsHide: true });
  const archive = readFileSync(zipped);
  const digest = createHash('sha256').update(archive).digest('hex');
  const requests: string[] = [];
  const unexpected: string[] = [];
  let source = '';
  const server = createServer((req, res) => {
    const path = req.url ?? '/';
    requests.push(path);
    assert.equal(req.socket.remoteAddress, '127.0.0.1');
    if (path === '/releases/latest' || path === '/') {
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify({ tag_name: 'setup-replay', assets: [{ name: 'codex-x86_64-pc-windows-msvc.exe.zip',
        browser_download_url: `${source}/codex.zip`, digest: `sha256:${digest}` }] }));
    } else if (path === '/codex.zip') {
      res.setHeader('Content-Type', 'application/zip');
      res.end(archive);
    } else {
      unexpected.push(path);
      res.writeHead(404).end();
    }
  });
  t.after(async () => {
    server.closeAllConnections();
    if (server.listening) await new Promise<void>((done) => server.close(() => done()));
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  source = `http://127.0.0.1:${address.port}`;
  const runs: Run[] = [];
  const records = (): Array<Command & { exited?: number }> => {
    const path = join(state, 'commands.jsonl');
    return existsSync(path) ? readFileSync(path, 'utf8').trim().split('\n').filter(Boolean).map((line) => JSON.parse(line)) : [];
  };
  const commands = (): Command[] => records().filter((r) => r.exited === undefined);
  // Only CLIs that never recorded their exit can still be running; a finished one's id may now belong to anything.
  const running = (): Command[] => {
    const gone = new Set(records().map((r) => r.exited).filter((pid) => pid !== undefined));
    return commands().filter((c) => !gone.has(c.pid));
  };
  async function launch(): Promise<Run> {
    assert.ok(existsSync(binary), 'build the real app with npm run test:e2e before running setup acceptance proofs');
    const port = 9300 + Math.floor(Math.random() * 1000);
    const env: NodeJS.ProcessEnv = { ...originalEnv };
    // Drop inherited provider and test overrides. Child PATH never contains a real CLI directory.
    for (const key of Object.keys(env)) if (/^(FABER_|CLAUDE|CODEX|OPENAI|ANTHROPIC|GEMINI|GOOGLE_API|HTTP_PROXY|HTTPS_PROXY|ALL_PROXY|NO_PROXY)/i.test(key)) delete env[key];
    delete env.Path;
    Object.assign(env, {
      HOME: profile, USERPROFILE: profile, APPDATA: join(profile, 'AppData/Roaming'),
      LOCALAPPDATA: join(profile, 'AppData/Local'), TEMP: join(sandbox, 'tmp'), TMP: join(sandbox, 'tmp'),
      PATH: join(sandbox, 'path'), CODEX_HOME: join(profile, 'fake-home'),
      FABER_DATA_DIR: data, FABER_HIDDEN: '1', FABER_CDP_PORT: String(port), FABER_SETUP_SOURCE: source,
      FABER_CLI_CLAUDE: connected ? JSON.stringify({ command: process.execPath, prefix: [fake, join(recordings, 'claude.setup.json')] }) : '',
      // Empty means disabled, while absent allows discovery of the newly installed data/bin CLI.
      FABER_FAKE_NODE: process.execPath, FABER_FAKE_SCRIPT: fake,
      FABER_FAKE_RECORDING: join(recordings, 'codex.setup.json'), FABER_FAKE_STATE_DIR: state,
      WEBVIEW2_USER_DATA_FOLDER: join(sandbox, 'webview'),
      HTTP_PROXY: 'http://127.0.0.1:1', HTTPS_PROXY: 'http://127.0.0.1:1', ALL_PROXY: 'http://127.0.0.1:1', NO_PROXY: '127.0.0.1,localhost',
    });
    let stderr = '';
    const app = spawn(binary, [], { cwd: sandbox, env, shell: false, windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
    app.stderr?.on('data', (chunk) => { stderr += chunk.toString(); });
    let spawnError: Error | undefined;
    app.on('error', (error) => { spawnError = error; });
    assert.ok(app.pid, 'real app must start');
    let browser: Browser | undefined;
    let page: Page | undefined;
    let lastError = '';
    try {
      await eventually(async () => {
        if (spawnError) assert.fail(`real app launch failed: ${spawnError.message}`);
        if (app.exitCode !== null) assert.fail(`real app exited prematurely with code ${app.exitCode}: ${stderr}`);
        try {
          if (!browser) {
            browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
          }
          page = browser.contexts()[0]?.pages()[0];
          return Boolean(page);
        } catch (err: unknown) {
          lastError = err instanceof Error ? err.message : String(err);
          await browser?.close().catch(() => {});
          browser = undefined;
          return false;
        }
      }, () => `the real app must expose its WebView2 window (lastError: ${lastError}, port: ${port})`, 25_000);
      assert.ok(page && browser);
      page.setDefaultTimeout(5000);
      const run = { page, browser, app, pid: app.pid, seen: descendants(processes(), app.pid) };
      runs.push(run);
      return run;
    } catch (error) {
      if (app.exitCode === null) app.kill();
      await browser?.close();
      throw error;
    }
  }
  async function close(run: Run) {
    run.seen.push(...descendants(processes(), run.pid));
    if (run.app.exitCode === null && run.app.signalCode === null) {
      const exited = once(run.app, 'exit');
      run.app.kill();
      await exited;
    }
    await run.browser.close();
  }
  async function audit() {
    const rows = processes();
    const seen = runs.flatMap((run) => run.seen);
    const leaked = rows.filter((row) => seen.some((old) => row.pid === old.pid && row.born === old.born) ||
      running().some((command) => row.pid === command.pid || row.pid === command.ppid));
    assert.deepEqual(leaked.map(({ pid, name }) => ({ pid, name })), [], 'no guide, login, status, download, or app child may survive');
    for (const command of commands()) {
      assert.ok(contained(sandbox, command.cwd), `CLI wrote from an external working directory: ${command.cwd}`);
      if (Array.isArray(command.targets)) {
        for (const target of command.targets) assert.ok(contained(sandbox, target), `CLI wrote to an external file: ${target}`);
      }
    }
    assert.deepEqual(snapshot(canary), before, 'nothing may write outside the scenario sandbox');
    const outsideFiles: string[] = [];
    let child = canary.replace(/[/\\][^/\\]+$/, '');
    for (let depth = 0; depth < 4; depth++) {
      const parent = child.replace(/[/\\][^/\\]+$/, '');
      if (!parent || parent === child) break;
      outsideFiles.push(...Object.keys(snapshot(parent, child)));
      child = parent;
    }
    assert.deepEqual(outsideFiles, [], `write outside sandbox detected: ${outsideFiles.join(', ')}`);
    assert.deepEqual({ ...process.env }, originalEnv, 'the real process environment and PATH must stay unchanged');
    assert.equal(userPath(), originalPath, 'installation must not change the real user PATH');
    assert.deepEqual(unexpected, [], 'downloads must use only the fake loopback source');
  }
  async function dispose() {
    try {
      for (const run of runs) if (run.app.exitCode === null && run.app.signalCode === null) await close(run);
      const survivors = () => {
        const seen = runs.flatMap((run) => run.seen);
        return processes().filter((row) => seen.some((old) => old.pid === row.pid && old.born === row.born) ||
          running().some((command) => row.pid === command.pid || row.pid === command.ppid));
      };
      await eventually(() => survivors().length === 0, () => `app descendants and commands must exit after closing; still running: ${
        survivors().map((row) => `${row.name}#${row.pid}`).join(', ')}`, 15_000);
      await audit();
    } finally {
      // Record leaks as failures before cleaning them up, so they cannot affect the next scenario. Only processes this
      // scenario started are killed: a process id that Windows already gave to another program is left alone.
      const rows = processes();
      const seen = runs.flatMap((run) => run.seen);
      for (const row of rows.filter((r) => seen.some((old) => old.pid === r.pid && old.born === r.born))) {
        try { process.kill(row.pid); } catch { /* already exited */ }
      }
      for (const command of running()) {
        try { process.kill(command.pid); } catch { /* already exited */ }
      }
      await new Promise<void>((done) => server.close(() => done()));
      rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
    }
  }
  return { launch, close, dispose, audit, commands, requests, data, payload, state, boundary, sandbox, enclosing, observed, root };
}

const guide = (page: Page) => page.getByRole('dialog', { name: en['setup.title'], exact: true });
async function visible(locator: Locator, message: string) {
  await eventually(() => locator.isVisible(), message);
}
async function opened(page: Page) { await visible(guide(page), 'first launch must open the setup guide'); }
async function connected(page: Page, vendor: string) {
  const card = page.locator(`.setup-card[data-vendor="${vendor}"]`);
  await visible(card.getByText(en['setup.connected'], { exact: true }), `${vendor} must show Connected`);
  for (const key of ['setup.install', 'setup.signIn'] as const) {
    assert.equal(await card.getByRole('button', { name: en[key], exact: true }).count(), 0);
  }
}
async function finish(page: Page) {
  await guide(page).getByRole('button', { name: en['setup.done'], exact: true }).click();
  await eventually(async () => !(await guide(page).isVisible()), 'Done must close the guide');
}

test('C5.1: installed signed-in Claude connects without reinstalling and answers the first question', { skip }, async (t) => {
  const f = await fixture(t, true);
  try {
    const run = await f.launch();
    await opened(run.page);
    await guide(run.page).getByRole('button', { name: en['setup.claude'], exact: true }).click();
    await connected(run.page, 'claude');
    assert.deepEqual(f.requests, [], 'an existing CLI must not be downloaded again');
    assert.ok(f.commands().some((c) => c.args.join(' ') === 'auth status'));
    assert.equal(f.commands().some((c) => c.args.join(' ') === 'auth login'), false);
    await finish(run.page);
    await visible(run.page.getByRole('heading', { name: en['empty.chatTitle'] }), 'chat must be ready immediately');
    const composer = run.page.getByLabel(en['composer.message']);
    assert.equal(await composer.isDisabled(), false);
    await composer.fill('In one sentence: why is the sky blue?');
    await composer.press('Enter');
    await visible(run.page.getByRole('article', { name: en['answer.label'] }).getByText(/Rayleigh/), 'installed Claude must answer');
    await f.close(run);
  } finally { await f.dispose(); }
});

test('C5.2: missing ChatGPT installs from loopback, signs in and answers without reopening', { skip }, async (t) => {
  const f = await fixture(t);
  try {
    const run = await f.launch();
    await opened(run.page);
    await guide(run.page).getByRole('button', { name: en['setup.chatgpt'], exact: true }).click();
    const card = run.page.locator('.setup-card[data-vendor="codex"]');
    await visible(card.getByText(en['setup.notInstalled'], { exact: true }), 'ChatGPT must initially be absent');
    await card.getByRole('button', { name: en['setup.install'], exact: true }).click();
    await visible(card.getByRole('button', { name: en['setup.signIn'], exact: true }), 'downloaded executable must run and report signed out');
    assert.deepEqual(readFileSync(join(f.data, 'bin/codex.exe')), f.payload, 'the served executable must be the installed CLI');
    assert.ok(f.requests.includes('/codex.zip'));
    await card.getByRole('button', { name: en['setup.signIn'], exact: true }).click();
    await eventually(() => existsSync(join(f.state, 'codex.signed-in')), 'official fake login must complete');
    const again = card.getByRole('button', { name: en['setup.checkAgain'], exact: true });
    if (await again.isVisible()) await again.click();
    await connected(run.page, 'codex');
    assert.ok(f.commands().some((c) => c.args.join(' ') === 'login'));
    assert.ok(f.commands().some((c) => c.args.join(' ') === 'login status'));
    await finish(run.page);
    await visible(run.page.getByRole('heading', { name: en['empty.chatTitle'] }), 'chat must be ready immediately');
    const composer = run.page.getByLabel(en['composer.message']);
    assert.equal(await composer.isDisabled(), false);
    await composer.fill('In one sentence: why is the sky blue?');
    await composer.press('Enter');
    await visible(run.page.getByRole('article', { name: en['answer.label'] }).getByText(/scatter/), 'newly connected ChatGPT must answer in the same app');
    assert.equal(run.app.exitCode, null, 'the original app must still be running');
    assert.equal(run.app.pid, run.pid);
    await f.close(run);
  } finally { await f.dispose(); }
});

test('C5.3: None and Skip persist, show connection guidance and never reopen setup automatically', { skip }, async (t) => {
  for (const decision of ['none', 'skip'] as const) {
    const f = await fixture(t);
    try {
      let run = await f.launch();
      await opened(run.page);
      if (decision === 'none') {
        await guide(run.page).getByRole('button', { name: en['setup.none'], exact: true }).click();
        await finish(run.page);
      } else await guide(run.page).getByRole('button', { name: en['setup.skip'], exact: true }).click();
      for (let launch = 0; launch < 2; launch++) {
        await visible(run.page.getByRole('heading', { name: en['empty.noAiTitle'] }), 'no-subscription choice must reach the app');
        const body = await run.page.locator('.empty p').innerText();
        assert.equal(body, en['empty.noAiBody']);
        assert.match(body, /Settings/);
        assert.match(body, /Subscriptions/);
        assert.doesNotMatch(body, /restart|reopen|relaunch/i);
        assert.equal(await guide(run.page).count(), 0, 'saved choice must suppress the guide');
        await eventually(() => JSON.parse(readFileSync(join(f.data, 'settings.json'), 'utf8')).setup ===
          (decision === 'none' ? 'done' : 'skipped'), 'setup decision must be durable before closing');
        await f.close(run);
        if (launch === 0) run = await f.launch();
      }
      assert.deepEqual(f.requests, []);
      assert.deepEqual(f.commands(), [], 'None and Skip must not start a CLI');
    } finally { await f.dispose(); }
  }
});

test('C5.4: the open guide with no conversation uses at most 250 MB', { skip }, async (t) => {
  const f = await fixture(t);
  try {
    const run = await f.launch();
    await opened(run.page);
    const samples: number[] = [];
    for (let i = 0; i < 3; i++) {
      const tree = descendants(processes(), run.pid);
      run.seen.push(...tree);
      assert.ok(tree.some((p) => p.name.toLowerCase().includes('msedgewebview2')), 'measure the app and its WebView2 children together');
      const mb = tree.reduce((sum, row) => sum + Number(row.bytes ?? 0), 0) / 1024 / 1024;
      assert.ok(Number.isFinite(mb) && mb > 0, 'private working set must be a real positive measurement');
      samples.push(mb);
    }
    assert.ok(Math.max(...samples) <= 250, `open guide: ${samples.map((mb) => mb.toFixed(1)).join(', ')} MB; limit 250 MB`);
    assert.deepEqual(f.commands(), [], 'opening the guide must not keep an idle CLI alive');
    await f.close(run);
  } finally { await f.dispose(); }
});

test('C5.5: closing the guide leaves no setup child or external writes', { skip }, async (t) => {
  const f = await fixture(t, true);
  try {
    const run = await f.launch();
    await opened(run.page);
    await guide(run.page).getByRole('button', { name: en['setup.claude'], exact: true }).click();
    await connected(run.page, 'claude');
    // All preceding scenarios audit the same boundaries in finally, including failing flows.
    // This dedicated proof also closes while the guide itself is still open.
    await f.close(run);
    await eventually(() => !processes().some((row) => f.commands().some((c) => c.pid === row.pid || c.ppid === row.pid)),
      'status/login child processes must stop when the guide closes', 5000);
    await f.audit();
  } finally { await f.dispose(); }
});

test('R5.5: audit rejects writes outside the scenario temporary directory even when cwd is preserved', { skip }, async (t) => {
  const f = await fixture(t);
  const beyondObserved = join(f.root, 'external-write.txt');
  const testSuffix = join(f.enclosing, 'unauthorized.test.ts');
  const previousCwd = process.cwd();
  try {
    process.chdir(f.sandbox);
    writeFileSync(beyondObserved, 'unauthorized absolute write beyond observed root');
    await assert.rejects(f.audit(), /write outside sandbox/i, 'audit must reject absolute writes beyond observed root');
    rmSync(beyondObserved, { force: true });
    writeFileSync(testSuffix, 'unauthorized write named test.ts');
    await assert.rejects(f.audit(), /write outside sandbox/i, 'audit must reject external writes regardless of name');
  } finally {
    process.chdir(previousCwd);
    rmSync(beyondObserved, { force: true });
    rmSync(testSuffix, { force: true });
    await f.dispose();
  }
});

// Exercise the existing recordings in memory too: setup-command support must not change the
// replay protocol. These regressions need no Windows app, process, network, or real CLI.
for (const vendor of ['claude', 'codex']) test(`existing ${vendor} recordings still replay unchanged`, async () => {
  const dir = join(root, 'tests/fixtures', vendor);
  for (const filename of readdirSync(dir).filter((name) => name.endsWith('.jsonl'))) {
    const entries: Entry[] = readFileSync(join(dir, filename), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
    const proc = replay(entries);
    const output: Entry['msg'][] = [];
    proc.stdout.on('data', (chunk) => {
      for (const line of chunk.toString().trim().split('\n')) output.push(JSON.parse(line));
    });
    for (const entry of entries) if (entry.dir === 'in') {
      proc.stdin.write(JSON.stringify(entry.msg) + '\n');
      await new Promise<void>((done) => setImmediate(done));
    }
    assert.deepEqual(proc.mismatches, [], filename);
    assert.deepEqual(output, entries.filter((entry) => entry.dir === 'out').map((entry) => entry.msg), filename);
    proc.stdin.end();
  }
});
