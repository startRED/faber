import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = (name) => readFileSync(new URL(`../${name}`, import.meta.url), 'utf8');
const directives = (policy) => Object.fromEntries(policy.split(';').map((line) => {
  const [key, ...values] = line.trim().split(/\s+/);
  return [key, values];
}));

test('the Tauri screen allows just its local media origin for images and media', () => {
  const policy = JSON.parse(source('src-tauri/tauri.conf.json')).app.security.csp;
  const origin = 'faber-media://localhost';
  for (const mediaOrigin of [origin, 'http://faber-media.localhost']) {
    const csp = directives(policy.replaceAll(origin, mediaOrigin));
    assert.deepEqual(csp['img-src'], ["'self'", 'data:', 'blob:', mediaOrigin]);
    assert.deepEqual(csp['media-src'], ['data:', 'blob:', mediaOrigin]);
    assert.deepEqual(csp['frame-src'], ['faber-visual://localhost', 'http://faber-visual.localhost']);
    assert.equal(csp['connect-src'], undefined);
    assert.ok(csp['script-src'].every((part) => !part.includes('://')));
  }
  assert.match(source('src-tauri/src/main.rs'), /policy\.replace\(media::CUSTOM_ORIGIN, media::ORIGIN\)/);
});

test('the Electron screen only adds its own media origin to image and media rules', () => {
  const script = source('electron/main.ts');
  const policy = script.split('const CSP = [')[1].split('].join')[0];
  assert.match(policy, /img-src 'self' data: blob: \$\{MEDIA_ORIGIN\}/);
  assert.match(policy, /media-src data: blob: \$\{MEDIA_ORIGIN\}/);
  assert.match(policy, /frame-src \$\{VISUAL_ORIGIN\}/);
  assert.doesNotMatch(policy, /https?:|connect-src/);
});
