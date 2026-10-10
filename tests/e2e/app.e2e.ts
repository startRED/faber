import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { _electron as electron } from 'playwright-core';

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));

// Launches the built app offscreen with its own data folder. `claude` is a recorded session to replay, if any.
async function launch(claude?: string, dataDir = mkdtempSync(join(tmpdir(), 'faber-e2e-'))) {
  // Linux CI runners cannot use Chromium's setuid sandbox helper.
  const args = process.platform === 'linux' && process.env.CI ? ['.', '--no-sandbox'] : ['.'];
  const cli = claude ? JSON.stringify({ command: process.execPath, prefix: [here('../support/fake-cli.ts'), here(`../fixtures/claude/${claude}.jsonl`)] }) : '';
  const env = { ...process.env, FABER_HIDDEN: '1', FABER_DATA_DIR: dataDir, FABER_CLI_CLAUDE: cli, FABER_CLI_CODEX: '' };
  const app = await electron.launch({ args, env });
  const window = await app.firstWindow();
  return { app, window };
}

async function shot(window: Awaited<ReturnType<typeof launch>>['window'], name: string) {
  mkdirSync('test-results/e2e', { recursive: true });
  await window.screenshot({ path: `test-results/e2e/${name}.png` });
}

test('the app opens on an empty chat and fits the window', async () => {
  const { app, window } = await launch('chat');
  try {
    await window.getByRole('heading', { name: 'What can I help with?' }).waitFor();
    assert.match(await window.locator('.foot').innerText(), /Claude · v\d+\.\d+\.\d+/);
    const scrolls = await window.evaluate(() => document.documentElement.scrollHeight > innerHeight);
    assert.equal(scrolls, false, 'the window should not scroll as a whole');
    await shot(window, 'home');
  } finally {
    await app.close();
  }
});

test('a chat streams an answer with the model and the reason, and lands in the sidebar', async () => {
  const { app, window } = await launch('chat');
  try {
    await window.getByLabel('Message').fill('In one sentence: why is the sky blue?');
    await window.getByLabel('Message').press('Enter');
    const answer = window.getByRole('article', { name: 'Answer' });
    await answer.locator('.actions').waitFor();
    const text = await answer.innerText();
    assert.match(text, /Rayleigh/);
    assert.match(text, /Haiku 5\.5/);
    assert.match(text, /Auto: everyday default/);
    assert.match(text, /at API prices/);
    await window.locator('.list .item', { hasText: 'In one sentence: why is the sky blue?' }).waitFor();
    await shot(window, 'chat');
  } finally {
    await app.close();
  }
});

test('editing a message sends it as a branch and keeps the original conversation', async () => {
  const { app, window } = await launch('chat');
  try {
    const question = 'In one sentence: why is the sky blue?';
    await window.getByLabel('Message').fill(question);
    await window.getByLabel('Message').press('Enter');
    await window.getByRole('article', { name: 'Answer' }).locator('.actions').waitFor();
    await window.locator('.you').hover();
    await window.getByRole('button', { name: 'Edit message' }).click();
    await window.getByRole('button', { name: 'Send as a branch' }).click();
    await window.getByRole('article', { name: 'Answer' }).locator('.actions').waitFor();
    await window.waitForFunction(() => document.querySelectorAll('.list .item').length === 2);
    await shot(window, 'branch');
  } finally {
    await app.close();
  }
});

test('with no AI installed, the app says what to do', async () => {
  const { app, window } = await launch();
  try {
    await window.getByRole('heading', { name: 'No AI found' }).waitFor();
    assert.equal(await window.getByRole('button', { name: 'Send' }).isDisabled(), true);
  } finally {
    await app.close();
  }
});

test('appearance changes apply at once and are kept after reopening', async () => {
  const dataDir = mkdtempSync(join(tmpdir(), 'faber-e2e-'));
  const bg = (window: Awaited<ReturnType<typeof launch>>['window']) => window.evaluate(() => getComputedStyle(document.body).backgroundColor);
  let { app, window } = await launch('chat', dataDir);
  try {
    await window.getByRole('button', { name: 'Settings' }).click();
    await window.getByRole('heading', { name: 'Appearance' }).waitFor();
    await window.getByRole('button', { name: 'Light' }).click();
    assert.equal(await bg(window), 'rgb(246, 247, 249)');
    await window.getByRole('button', { name: 'Cockpit' }).click();
    assert.equal(await bg(window), 'rgb(0, 0, 0)');
    assert.equal(await window.getByRole('button', { name: 'Compact' }).getAttribute('aria-pressed'), 'true');
    await shot(window, 'settings-cockpit');
    await window.getByRole('button', { name: 'Projects' }).last().click();
    await window.getByRole('checkbox').check();
    // The file is written in the background; closing earlier could lose the last choice.
    const saved = () => existsSync(join(dataDir, 'settings.json')) && readFileSync(join(dataDir, 'settings.json'), 'utf8').includes('"userHooks": true');
    while (!saved()) await new Promise((r) => setTimeout(r, 20));
  } finally {
    await app.close();
  }
  ({ app, window } = await launch('chat', dataDir));
  try {
    await window.getByRole('heading', { name: 'What can I help with?' }).waitFor();
    await window.waitForFunction(() => getComputedStyle(document.body).backgroundColor === 'rgb(0, 0, 0)');
    await window.keyboard.press('Control+,');
    await window.getByRole('button', { name: 'Projects' }).last().click();
    assert.equal(await window.getByRole('checkbox').isChecked(), true);
  } finally {
    await app.close();
  }
});
