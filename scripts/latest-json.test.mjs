import assert from 'node:assert/strict';
import test from 'node:test';
import { PLATFORMS } from './channel.mjs';
import { merge } from './latest-json.mjs';

const entries = PLATFORMS.map((platform) => ({ platform, signature: `signature-${platform}`, url: `https://github.com/owner/repo/releases/download/v1.2.3/${platform}` }));
const make = (items = entries) => merge('1.2.3', 'notes', '2026-01-01T00:00:00Z', items);

test('merges all signed platform files into the updater format', () => {
  const result = make();
  assert.equal(result.version, '1.2.3');
  assert.equal(result.notes, 'notes');
  assert.equal(result.pub_date, '2026-01-01T00:00:00Z');
  for (const { platform, signature, url } of entries) assert.deepEqual(result.platforms[platform], { signature, url });
});

test('refuses missing, duplicate, unknown or unsigned platforms', () => {
  assert.throws(() => make(entries.slice(1)));
  assert.throws(() => make([...entries, entries[0]]));
  assert.throws(() => make([...entries, { ...entries[0], platform: 'other' }]));
  assert.throws(() => make([{ ...entries[0], signature: '' }, ...entries.slice(1)]));
  assert.throws(() => make([{ ...entries[0], url: 'http://example.com/file' }, ...entries.slice(1)]));
  assert.throws(() => merge('bad', '', '2026-01-01', entries));
});
