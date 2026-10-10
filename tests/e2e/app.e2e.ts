import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { _electron as electron } from 'playwright-core';

// Launches the real built app (run `npm run build` first), hidden, and saves what a judge would see.
test('the app opens and shows its name and version', async () => {
  // Linux CI runners cannot use Chromium's setuid sandbox helper.
  const args = process.platform === 'linux' && process.env.CI ? ['.', '--no-sandbox'] : ['.'];
  const app = await electron.launch({ args, env: { ...process.env, FABER_HIDDEN: '1' } });
  try {
    const window = await app.firstWindow();
    await window.getByRole('heading', { name: 'Faber' }).waitFor();
    const text = await window.locator('main').innerText();
    assert.match(text, /v\d+\.\d+\.\d+/);
    const scrolls = await window.evaluate(() => document.documentElement.scrollHeight > innerHeight);
    assert.equal(scrolls, false, 'the home screen should fit without a scrollbar');
    mkdirSync('test-results/e2e', { recursive: true });
    await window.screenshot({ path: 'test-results/e2e/home.png' });
  } finally {
    await app.close();
  }
});
