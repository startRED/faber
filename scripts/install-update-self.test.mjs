import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { retained, sample } from './install-update-self.mjs';

test('retention requires the running-app message and setting, not just a file', () => {
  retained(sample(), { language: 'en' });
  assert.throws(() => retained({ ...sample(), messages: [] }, { language: 'en' }));
  assert.throws(() => retained(sample(), { language: 'pt-BR' }));
  assert.throws(() => retained({ ...sample(), id: 'other' }, { language: 'en' }));
});

test('three disposable Windows jobs run only on dispatch or release, retaining failures', () => {
  const workflow = readFileSync(new URL('../.github/workflows/install-update-self.yml', import.meta.url), 'utf8');
  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /release:\s+types: \[published\]/);
  assert.match(workflow, /scenario: \[tauri, clean, electron\]/);
  assert.match(workflow, /runs-on: windows-latest/);
  assert.match(workflow, /if: always\(\)/);
  assert.doesNotMatch(workflow, /schedule:|pull_request:|push:/);
  const script = readFileSync(new URL('./install-update-self.mjs', import.meta.url), 'utf8');
  assert.doesNotMatch(script, /taskkill|Stop-Process|process\.kill|remote-debugging|--inspect/);
  assert.match(script, /call\('update_restart'\)/);
  assert.match(script, /call\('conversation'/);
  assert.match(script, /conversation-open-before/);
});
