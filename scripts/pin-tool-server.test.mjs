import assert from 'node:assert/strict';
import { gzipSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { checkPackageFiles, curateCandidate, packageTreeDigest, registryUrl, satisfies, unpackPackage } from './pin-tool-server.mjs';

function makeTar(entries) {
  const chunks = [];
  for (const { name, body, type = '0', link = '' } of entries) {
    const header = Buffer.alloc(512);
    header.write(name, 0, 100);
    header.write('0000644\0', 100, 8);
    header.write('0000000\0', 108, 8);
    header.write('0000000\0', 116, 8);
    header.write(`${body.length.toString(8).padStart(11, '0')}\0`, 124, 12);
    header.write('00000000000\0', 136, 12);
    header.fill(0x20, 148, 156);
    header.write(type, 156, 1);
    header.write(link, 157, 100);
    header.write('ustar\0', 257, 6);
    header.write('00', 263, 2);
    let sum = 0;
    for (const byte of header) sum += byte;
    header.write(`${sum.toString(8).padStart(6, '0')}\0 `, 148, 8);
    chunks.push(header, body, Buffer.alloc((512 - body.length % 512) % 512));
  }
  chunks.push(Buffer.alloc(1024));
  return gzipSync(Buffer.concat(chunks));
}

test('registry URLs require the exact HTTPS registry authority', () => {
  assert.equal(registryUrl('https://registry.npmjs.org/pkg/-/pkg-1.0.0.tgz').hostname, 'registry.npmjs.org');
  for (const url of [
    'http://registry.npmjs.org/pkg',
    'https://registry.npmjs.org:443/pkg',
    'https://user@registry.npmjs.org/pkg',
    'https://registry.npmjs.org.attacker.invalid/pkg',
    'https://example.invalid/pkg',
  ]) assert.throws(() => registryUrl(url));
});

test('supported semver ranges choose stable compatible versions', () => {
  assert.equal(satisfies('1.11.0', '^1.10.0'), true);
  assert.equal(satisfies('2.0.0', '^1.10.0'), false);
  assert.equal(satisfies('1.4.9', '~1.4.0'), true);
  assert.equal(satisfies('1.5.0', '~1.4.0'), false);
  assert.equal(satisfies('2.0.0', '>=1.0.0 <3.0.0'), true);
  assert.equal(satisfies('0.2.9', '^0.2.3'), true);
  assert.equal(satisfies('0.3.0', '^0.2.3'), false);
  assert.equal(satisfies('1.2.4-alpha.2', '^1.2.4'), false);
  assert.equal(satisfies('1.2.4-alpha.2', '^1.2.4-alpha.1'), true);
  assert.equal(satisfies('1.2.4', '>=999 *'), false);
  assert.equal(satisfies('1.2.4', 'workspace:*'), false);
});

test('package extraction rejects traversal and links', () => {
  assert.throws(() => unpackPackage(makeTar([{ name: 'package/../escape', body: Buffer.from('x') }]), 'pkg', '1.0.0'), /path/);
  assert.throws(() => unpackPackage(makeTar([{ name: 'package/link', body: Buffer.alloc(0), type: '2', link: 'target' }]), 'pkg', '1.0.0'), /plain file/);
});

test('package extraction requires exact package manifest identity', () => {
  const archive = makeTar([
    { name: 'package/package.json', body: Buffer.from('{"name":"pkg","version":"1.0.0"}') },
    { name: 'package/index.js', body: Buffer.from('export {};') },
  ]);
  assert.equal(unpackPackage(archive, 'pkg', '1.0.0').fileCount, 2);
  assert.throws(() => unpackPackage(archive, 'pkg', '2.0.0'), /identity mismatch/);
});

test('package extraction accepts one portable root folder and rejects multiple roots', () => {
  const contents = Buffer.from('{"name":"pkg","version":"1.0.0"}');
  assert.equal(unpackPackage(makeTar([{ name: 'node v22.20/package.json', body: contents }]), 'pkg', '1.0.0').fileCount, 1);
  assert.throws(() => unpackPackage(makeTar([
    { name: 'one/package.json', body: contents },
    { name: 'two/index.js', body: Buffer.from('') },
  ]), 'pkg', '1.0.0'), /Multiple top-level roots/);
});

test('digest matches Faber path-NUL-length-NUL-bytes ordering', () => {
  const files = new Map([
    ['b.txt', Buffer.from('B')],
    ['a.txt', Buffer.from('A')],
  ]);
  assert.equal(packageTreeDigest(files), 'ca5630fa03fe09881b895e1620040cd3f2a7dfc3a62397322f50719d242156a3');
  assert.equal(packageTreeDigest(new Map([
    ['😀', Buffer.from('e')],
    ['é', Buffer.from('b')],
    ['a', Buffer.from('a')],
  ])), 'd5258fe714f1868782dc73ed37243853a1918f7abb6475216d9ff6df72ee1bfc');
});

test('lifecycle scripts are found even when empty and native artifacts are listed', () => {
  const files = new Map([
    ['package.json', { path: 'package.json', bytes: Buffer.from('{"scripts":{"install":""}}') }],
    ['build/binding.gyp', { path: 'build/binding.gyp', bytes: Buffer.from('{}') }],
    ['native/addon.node', { path: 'native/addon.node', bytes: Buffer.from([0]) }],
  ]);
  assert.deepEqual(checkPackageFiles(files), {
    lifecycle: [{ hook: 'install', value: '' }],
    native: ['build/binding.gyp', 'native/addon.node'],
  });
});

test('curation pins a complete two-level tree and refuses changed archive bytes', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'faber-tree-curation-'));
  const originalFetch = globalThis.fetch;
  const responses = new Map();
  const files = new Map();
  for (const [name, dependencies, location] of [
    ['root', { middle: '^1.0.0' }, ''],
    ['middle', { leaf: '1.0.0' }, 'node_modules/middle/'],
    ['leaf', {}, 'node_modules/leaf/'],
  ]) {
    const manifest = { name, version: '1.0.0', dependencies };
    const body = Buffer.from(JSON.stringify(manifest));
    const archive = makeTar([{ name: 'package/package.json', body }]);
    const url = `https://registry.npmjs.org/${name}/-/${name}-1.0.0.tgz`;
    const metadata = { ...manifest, dist: { tarball: url, integrity: `sha512-${createHash('sha512').update(archive).digest('base64')}` } };
    responses.set(url, archive);
    responses.set(`https://registry.npmjs.org/${name}`, JSON.stringify({ versions: { '1.0.0': metadata } }));
    responses.set(`https://registry.npmjs.org/${name}/1.0.0`, JSON.stringify(metadata));
    files.set(`${location}package.json`, body);
  }
  globalThis.fetch = async (url, options) => {
    assert.equal(options.redirect, 'error');
    assert(responses.has(String(url)), `unexpected source: ${url}`);
    return new Response(responses.get(String(url)));
  };
  try {
    const report = await curateCandidate('root', '1.0.0', directory);
    assert.equal(report.treeComplete, true);
    assert.equal(report.technicalStatus, 'eligible');
    assert.deepEqual(report.tree.map(pin => pin.path), ['', 'node_modules/leaf', 'node_modules/middle']);
    assert.equal(report.treeDigest, packageTreeDigest(files));
    responses.set('https://registry.npmjs.org/leaf/-/leaf-1.0.0.tgz', Buffer.from('changed'));
    const badDirectory = path.join(directory, 'bad-integrity');
    const refused = await curateCandidate('root', '1.0.0', badDirectory);
    assert.equal(refused.admitted, false);
    assert.equal(refused.treeComplete, false);
    assert(refused.reason.some(reason => reason.includes('SHA-512 integrity mismatch')));
  } finally {
    globalThis.fetch = originalFetch;
    assert.equal(path.dirname(path.resolve(directory)), path.resolve(tmpdir()));
    assert(path.basename(directory).startsWith('faber-tree-curation-'));
    await rm(directory, { recursive: true, force: true });
  }
});
