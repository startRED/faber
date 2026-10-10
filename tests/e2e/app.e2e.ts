import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, type Page } from 'playwright-core';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));
const BIN = here(`../../target/e2e/debug/faber${process.platform === 'win32' ? '.exe' : ''}`);
// Only WebView2 (Windows) speaks the DevTools protocol; macOS and Linux webviews cannot be driven this way.
const skip = process.platform !== 'win32' && 'needs WebView2';
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Starts the built app offscreen with its own data folder, in English unless told otherwise.
// `claude` is a recorded session to replay through the fake CLI, if any.
async function launch(claude?: string, dataDir = mkdtempSync(join(tmpdir(), 'faber-e2e-')), language = 'en') {
  const settings = join(dataDir, 'settings.json');
  if (!existsSync(settings)) writeFileSync(settings, JSON.stringify({ language }));
  const port = 9300 + Math.floor(Math.random() * 600);
  const cli = claude ? JSON.stringify({ command: process.execPath, prefix: [here('../support/fake-cli.ts'), here(`../fixtures/claude/${claude}.jsonl`)] }) : '';
  const env = { ...process.env, FABER_HIDDEN: '1', FABER_DATA_DIR: dataDir, FABER_CLI_CLAUDE: cli, FABER_CLI_CODEX: '', FABER_CDP_PORT: String(port) };
  const app = spawn(BIN, [], { env, stdio: 'ignore' });
  let page: Page | undefined;
  for (let i = 0; i < 200 && !page; i++) {
    await sleep(100);
    try {
      const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      page = browser.contexts()[0]?.pages()[0];
    } catch {
      // still starting
    }
  }
  assert.ok(page, 'the app did not open its window');
  await page.locator('.app').waitFor();
  const close = async () => {
    app.kill();
    await once(app, 'exit');
  };
  return { page, close, dataDir, pid: app.pid! };
}

// What Faber may use with no CLI running, as Task Manager counts it (private working set of the app and its webview).
// The product limit is 500 MB when Faber is complete; the test keeps half of it as room for what is still to come.
const BUDGET_MB = 250;

function memoryMb(root: number): number {
  const ps = `$all = Get-CimInstance Win32_Process; function T($i) { $i; $all | ? { $_.ParentProcessId -eq $i } | % { T $_.ProcessId } }; $ids = @(T ${root}); ` +
    `$w = Get-CimInstance Win32_PerfRawData_PerfProc_Process | ? { $ids -contains [int]$_.IDProcess }; [math]::Round(($w | Measure-Object WorkingSetPrivate -Sum).Sum / 1MB)`;
  return Number(execFileSync('powershell.exe', ['-NoProfile', '-Command', ps], { maxBuffer: 1 << 20 }).toString().trim());
}

async function shot(page: Page, name: string) {
  mkdirSync('test-results/e2e', { recursive: true });
  await page.screenshot({ path: `test-results/e2e/${name}.png` });
}

test('the app opens on an empty chat and fits the window', { skip }, async () => {
  const { page, close } = await launch('chat');
  try {
    await page.getByRole('heading', { name: en['empty.chatTitle'] }).waitFor();
    assert.match(await page.locator('.foot').innerText(), /Claude · v\d+\.\d+\.\d+/);
    assert.equal(await page.evaluate(() => document.documentElement.scrollHeight > innerHeight), false, 'the window should not scroll as a whole');
    await shot(page, 'home');
  } finally {
    await close();
  }
});

test('a chat streams an answer with the model and the reason, and lands in the sidebar', { skip }, async () => {
  const { page, close } = await launch('chat');
  try {
    const question = 'In one sentence: why is the sky blue?';
    await page.getByLabel(en['composer.message']).fill(question);
    await page.getByLabel(en['composer.message']).press('Enter');
    const answer = page.getByRole('article', { name: en['answer.label'] });
    await answer.locator('.actions').waitFor();
    const text = await answer.innerText();
    assert.match(text, /Rayleigh/);
    assert.match(text, /Haiku 5\.5/);
    assert.ok(text.includes(en['reason.autoDefault']), text);
    assert.match(text, /at API prices/);
    await page.locator('.list .item', { hasText: question }).waitFor();
    await shot(page, 'chat');
  } finally {
    await close();
  }
});

test('editing a message sends it as a branch and keeps the original conversation', { skip }, async () => {
  const { page, close } = await launch('chat');
  try {
    await page.getByLabel(en['composer.message']).fill('In one sentence: why is the sky blue?');
    await page.getByLabel(en['composer.message']).press('Enter');
    await page.getByRole('article', { name: en['answer.label'] }).locator('.actions').waitFor();
    await page.locator('.you').hover();
    await page.getByRole('button', { name: en['mine.editLabel'] }).click();
    await page.getByRole('button', { name: en['mine.sendBranch'] }).click();
    await page.getByRole('article', { name: en['answer.label'] }).locator('.actions').waitFor();
    await page.waitForFunction(() => document.querySelectorAll('.list .item').length === 2);
    await shot(page, 'branch');
  } finally {
    await close();
  }
});

test('deleting a conversation asks once, then removes it', { skip }, async () => {
  const { page, close } = await launch('chat');
  try {
    await page.getByLabel(en['composer.message']).fill('In one sentence: why is the sky blue?');
    await page.getByLabel(en['composer.message']).press('Enter');
    await page.getByRole('article', { name: en['answer.label'] }).locator('.actions').waitFor();
    await page.locator('.list .item').hover();
    const x = page.locator('.list .item .x');
    await x.click();
    assert.equal(await x.innerText(), en['side.confirmDelete']);
    await x.click();
    await page.waitForFunction(() => document.querySelectorAll('.list .item').length === 0);
  } finally {
    await close();
  }
});

test('with no AI installed, the app says what to do', { skip }, async () => {
  const { page, close } = await launch();
  try {
    await page.getByRole('heading', { name: en['empty.noAiTitle'] }).waitFor();
    assert.equal(await page.getByRole('button', { name: en['composer.send'] }).isDisabled(), true);
  } finally {
    await close();
  }
});

test('the screen speaks the chosen language and switches at once', { skip }, async () => {
  const { page, close } = await launch('chat', undefined, 'pt-BR');
  try {
    await page.getByRole('heading', { name: ptBR['empty.chatTitle'] }).waitFor();
    assert.equal(await page.evaluate(() => document.documentElement.lang), 'pt-BR');
    await shot(page, 'home-pt-BR');
    await page.getByRole('button', { name: ptBR['settings.open'] }).click();
    await page.getByLabel(ptBR['set.language']).selectOption('es');
    await page.getByRole('heading', { name: es['set.general'] }).waitFor();
    assert.equal(await page.evaluate(() => document.documentElement.lang), 'es');
  } finally {
    await close();
  }
});

test('appearance changes apply at once and are kept after reopening', { skip }, async () => {
  const bg = (page: Page) => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  let { page, close, dataDir } = await launch('chat');
  try {
    await page.getByRole('button', { name: en['settings.open'] }).click();
    await page.getByRole('button', { name: en['set.appearance'] }).click();
    await page.getByRole('heading', { name: en['set.appearance'] }).waitFor();
    await page.getByRole('button', { name: en['set.modeLight'] }).click();
    assert.equal(await bg(page), 'rgb(246, 247, 249)');
    await page.getByRole('button', { name: 'Cockpit' }).click();
    assert.equal(await bg(page), 'rgb(0, 0, 0)');
    assert.equal(await page.getByRole('button', { name: en['set.densityCompact'] }).getAttribute('aria-pressed'), 'true');
    await shot(page, 'settings-cockpit');
    await page.getByRole('button', { name: en['set.projects'] }).last().click();
    await page.getByRole('checkbox').check();
    // The file is written in the background; closing earlier could lose the last choice.
    const saved = () => readFileSync(join(dataDir, 'settings.json'), 'utf8').includes('"userHooks": true');
    while (!saved()) await sleep(20);
  } finally {
    await close();
  }
  ({ page, close } = await launch('chat', dataDir));
  try {
    await page.getByRole('heading', { name: en['empty.chatTitle'] }).waitFor();
    await page.waitForFunction(() => getComputedStyle(document.body).backgroundColor === 'rgb(0, 0, 0)');
    // An unfocused offscreen window reopened on the same profile only takes keys after a click, like a real one.
    await page.getByLabel(en['composer.message']).click();
    await page.keyboard.press('Control+,');
    await page.getByRole('heading', { name: en['set.general'] }).waitFor();
    await page.getByRole('button', { name: en['set.projects'] }).last().click();
    assert.equal(await page.getByRole('checkbox').isChecked(), true);
  } finally {
    await close();
  }
});

test('with no CLI running, Faber stays within its memory budget', { skip }, async () => {
  const { page, close, pid } = await launch();
  try {
    await page.getByRole('heading', { name: en['empty.noAiTitle'] }).waitFor();
    await sleep(2000);
    const mb = memoryMb(pid);
    console.log(`idle: ${mb} MB`);
    assert.ok(mb <= BUDGET_MB, `${mb} MB is over the ${BUDGET_MB} MB budget`);
  } finally {
    await close();
  }
});

test('a conversation with a thousand messages opens quickly and stays within the budget', { skip }, async () => {
  const dataDir = mkdtempSync(join(tmpdir(), 'faber-e2e-'));
  const id = randomUUID();
  const at = Date.now();
  const paragraph = 'Rayleigh scattering sends **short blue wavelengths** in every direction, so the sky looks blue. ';
  const fence = '`'.repeat(3);
  const answer = (i: number) => [paragraph.repeat(6), '- one\n- two', `${fence}js\nconsole.log(${i});\n${fence}`].join('\n\n');
  const messages = Array.from({ length: 1000 }, (_, i) =>
    i % 2 === 0
      ? { id: randomUUID(), role: 'user', text: `Question ${i / 2}: why is the sky blue?`, at: at + i }
      : { id: randomUUID(), role: 'assistant', text: answer(i), at: at + i, vendor: 'claude', model: 'claude-haiku-5-5', status: 'done' },
  );
  mkdirSync(join(dataDir, 'conversations'));
  writeFileSync(join(dataDir, 'conversations', `${id}.json`), JSON.stringify({ id, title: 'Long one', area: 'chat', sessions: {}, createdAt: at, updatedAt: at, messages }));
  const { page, close, pid } = await launch('chat', dataDir);
  try {
    const started = Date.now();
    await page.locator('.list .item', { hasText: 'Long one' }).click();
    await page.locator('article.ai').last().waitFor();
    const openMs = Date.now() - started;
    const drawn = await page.locator('article.ai').count();
    assert.ok(drawn <= 25, `${drawn} answers drawn at once`);
    // Scrolling up draws the earlier messages, keeping the reader's place.
    await page.locator('.thread').evaluate((el) => (el.scrollTop = 0));
    await page.waitForFunction((n) => document.querySelectorAll('article.ai').length > n, drawn);
    await sleep(2000);
    const mb = memoryMb(pid);
    console.log(`1000 messages: opened in ${openMs} ms, ${mb} MB`);
    assert.ok(openMs < 3000, `opening took ${openMs} ms`);
    assert.ok(mb <= BUDGET_MB, `${mb} MB is over the ${BUDGET_MB} MB budget`);
  } finally {
    await close();
  }
});
