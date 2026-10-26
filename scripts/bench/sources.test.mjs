import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('../../bench/proofs/sources.mjs', import.meta.url));
// Exercise the real proof entry point without any network requests.
const offline = 'data:text/javascript,' + encodeURIComponent(`
  globalThis.fetch = async (url) => new Response('', { status: url.endsWith('/missing') ? 404 : 200 });
`);

function proof(count, missing = false) {
  const dir = mkdtempSync(join(tmpdir(), 'faber-source-proof-'));
  try {
    const answer = join(dir, 'answer.md');
    const urls = Array.from({ length: count }, (_, i) => `https://nodejs.org/${missing && i === count - 1 ? 'missing' : i}`);
    writeFileSync(answer, urls.map((url) => `[source](${url})`).join('\n'));
    return JSON.parse(execFileSync(process.execPath, ['--import', offline, script, dir, dir, answer], { encoding: 'utf8', windowsHide: true }));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('source proof does not approve an unchecked link beyond its request limit', () => {
  const result = proof(21, true);
  assert.equal(result.pass, false, 'the missing 21st source was not checked and cannot qualify');
  assert.ok(result.checks.some((check) => !check.ok && check.detail.includes('20 of 21')));
});

test('source proof accepts every reachable link through its request limit', () => {
  for (const count of [4, 20]) assert.equal(proof(count).pass, true, `${count} reachable sources`);
});

test('source proof still rejects a missing source within the request limit', () => {
  const result = proof(4, true);
  assert.equal(result.pass, false);
  assert.ok(result.checks.some((check) => !check.ok && check.detail.includes('404 https://nodejs.org/missing')));
});
