import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import config from '../vite.config.ts';

test('the built screen starts only through the exact hash-authorized loader, without a host nonce', () => {
  const plugin = config.plugins.find((p) => p.name === 'visual-runtime');
  assert.equal(plugin.transformIndexHtml.order, 'post', 'Vite must finish generating the entries first');
  const made = plugin.transformIndexHtml.handler('<head><script type="module" src="./assets/entry.js"></script><script type="module">import "./refresh.js"</script><link rel="modulepreload" crossorigin href="./assets/shared.js"></head>');
  assert.equal(made.html.match(/application\/faber-module/g).length, 2);
  assert.ok(!made.html.includes('type="module"'), 'parser entries must stay inert, without a deliberate CSP violation');
  assert.ok(!made.html.includes('modulepreload'), 'parser preloads must not violate the inherited strict script policy either');
  const hash = createHash('sha256').update(made.tags[0].children).digest('base64');
  const security = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')).app.security;
  assert.ok(security.csp.includes(`'sha256-${hash}'`), 'the injected bytes must match the host policy');
  assert.ok(!security.csp.includes('nonce-'));
  assert.deepEqual(security.dangerousDisableAssetCspModification, ['script-src']);
});
