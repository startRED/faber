import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('the installed Tauri shell cannot read the debugging port environment variable', () => {
  const main = readFileSync(new URL('../src-tauri/src/main.rs', import.meta.url), 'utf8');
  assert.ok(/#\[cfg\(feature = "test-hooks"\)\]\s*let port = std::env::var\("FABER_CDP_PORT"\)/.test(main), 'CDP environment access must be compiled only with test-hooks');
  assert.ok(/#\[cfg\(not\(feature = "test-hooks"\)\)\]\s*let port: Option<String> = None;/.test(main), 'production must have no CDP port');
});
