import { test } from 'node:test';
import assert from 'node:assert/strict';
import { publicAddress, publicHttps } from '../electron/visualrules.ts';

test('visual pages reach only public HTTPS destinations', () => {
  for (const address of ['8.8.8.8', '1.1.1.1', '2606:4700:4700::1111', '::ffff:8.8.8.8']) assert.equal(publicAddress(address), true, address);
  for (const address of ['0.0.0.0', '10.0.0.1', '100.64.0.1', '127.0.0.1', '169.254.1.1', '172.16.1.1', '192.0.2.1', '192.168.1.1', '198.18.1.1', '198.51.100.1', '203.0.113.1', '224.0.0.1', '240.0.0.1', '::1', 'fc00::1', 'fe80::1', 'ff02::1', '2001:db8::1', '2002:7f00:1::1', '3fff::1', '::ffff:127.0.0.1']) assert.equal(publicAddress(address), false, address);
  for (const url of ['https://localhost/', 'https://x.localhost/', 'https://x.local/', 'https://x.internal/', 'http://example.org/', 'https://user@example.org/']) assert.equal(publicHttps(url), false, url);
  assert.equal(publicHttps('https://example.org/'), true);
});
