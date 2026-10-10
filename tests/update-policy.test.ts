import { test } from 'node:test';
import assert from 'node:assert/strict';
import { updateMode } from '../src/main/update-policy.ts';

test('macOS only notifies, because unsigned updates cannot be installed there', () => {
  assert.equal(updateMode('darwin'), 'notify');
});

test('Windows and Linux install updates by themselves', () => {
  assert.equal(updateMode('win32'), 'install');
  assert.equal(updateMode('linux'), 'install');
});
