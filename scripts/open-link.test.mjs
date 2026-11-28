import assert from 'node:assert/strict';
import { appendFileSync, existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const main = readFileSync(new URL('../electron/main.ts', import.meta.url), 'utf8');
const source = main.slice(main.indexOf('async function openLink('), main.indexOf('/** Commands only a window'));
const mode = readFileSync(new URL('../electron/testmode.ts', import.meta.url), 'utf8').match(/export const testing = ([^;]+);/)[1];

function fixture(t, packaged = false, hidden = '1') {
  const dataDir = mkdtempSync(path.join(tmpdir(), 'faber-open-link-'));
  t.after(() => rmSync(dataDir, { recursive: true, force: true }));
  const opened = [];
  const testing = runInNewContext(mode, { app: { isPackaged: packaged }, process: { env: { FABER_HIDDEN: hidden } } });
  const open = runInNewContext(`${stripTypeScriptTypes(source)}\nopenLink`, {
    URL, path, dataDir, appendFileSync, testmode: { testing }, shell: { openExternal: async (url) => { opened.push(url); } },
  });
  return { open, opened, log: path.join(dataDir, 'blocked-opens.jsonl') };
}

test('open-link: hidden test shell records each URL without the OS opener', async (t) => {
  const { open, opened, log } = fixture(t);
  await open({ url: 'https://example.com/a?q=%22test%22#part' });
  await open({ url: 'http://example.org' });
  assert.deepEqual(opened, []);
  assert.deepEqual(readFileSync(log, 'utf8').trim().split('\n').map(JSON.parse), [
    'https://example.com/a?q=%22test%22#part', 'http://example.org/',
  ]);
});

test('open-link: packaged and ordinary development shells keep the OS opener', async (t) => {
  for (const [packaged, hidden] of [[true, '1'], [false, '0']]) {
    const { open, opened, log } = fixture(t, packaged, hidden);
    await open({ url: 'https://example.com' });
    assert.deepEqual(opened, ['https://example.com/']);
    assert.equal(existsSync(log), false);
  }
});

test('open-link: invalid links and failed recording never reach the OS opener', async (t) => {
  const { open, opened, log } = fixture(t);
  for (const url of ['not a URL', 'file:///C:/Windows', 'javascript:alert(1)']) {
    await assert.rejects(open({ url }), /error.link.web/);
  }
  assert.equal(existsSync(log), false);
  rmSync(path.dirname(log), { recursive: true });
  await assert.rejects(open({ url: 'https://example.com' }), /error.link.open/);
  assert.deepEqual(opened, []);
});
