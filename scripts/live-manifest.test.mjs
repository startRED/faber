// Offline signature boundary only: generated keys are disposable test fixtures, never account or production keys.
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import test from 'node:test';
import { MAX_ENVELOPE_BYTES, parsePayload, verifyEnvelope } from './live-manifest.mjs';

const { publicKey, privateKey } = generateKeyPairSync('ed25519');
const trusted = new Map([['fixture', publicKey]]);
const envelopeFor = (payload) => ({
  key_id: 'fixture', payload: payload.toString('base64'), signature: sign(null, payload, privateKey).toString('base64'),
});

test('the publication contract verifies exact bytes with a pinned public key', () => {
  const payload = Buffer.from('{"schema":1,"revision":1,"name":"ação"}\n');
  assert.deepEqual(verifyEnvelope(JSON.stringify(envelopeFor(payload)), trusted), payload);
});

test('changed bytes, signatures and untrusted keys cannot authenticate a document', () => {
  const envelope = envelopeFor(Buffer.from('{"revision":1}'));
  const changedSignature = Buffer.from(envelope.signature, 'base64');
  changedSignature[0] ^= 1;
  for (const changed of [
    { ...envelope, payload: Buffer.from('{"revision":2}').toString('base64') },
    { ...envelope, signature: changedSignature.toString('base64') },
    { ...envelope, signature: Buffer.alloc(63).toString('base64') },
    { ...envelope, key_id: 'unknown' },
  ]) assert.throws(() => verifyEnvelope(JSON.stringify(changed), trusted));
  const other = generateKeyPairSync('ed25519');
  assert.throws(() => verifyEnvelope(JSON.stringify(envelope), new Map([['fixture', other.publicKey]])));
  assert.throws(() => verifyEnvelope(JSON.stringify(envelope), new Map([['fixture', privateKey]])));
});

test('malformed and oversized envelopes are refused before authentication', () => {
  const envelope = envelopeFor(Buffer.from('{}'));
  for (const bad of [null, {}, '', '{', 'null', '[]', '3', JSON.stringify({ ...envelope, public_key: 'untrusted' }),
    JSON.stringify({ payload: envelope.payload, signature: envelope.signature }),
    JSON.stringify({ ...envelope, key_id: '__proto__' }), ' '.repeat(MAX_ENVELOPE_BYTES + 1),
    'é'.repeat(MAX_ENVELOPE_BYTES / 2 + 1),
  ]) assert.throws(() => verifyEnvelope(bad, trusted));
});

test('base64 must be canonical rather than silently repaired by the decoder', () => {
  const envelope = envelopeFor(Buffer.from('{}'));
  for (const field of ['payload', 'signature']) {
    for (const value of ['', '!', 'Zg', 'Zg===', 'Zh==', ' Zg==', 'Zg==\n', 12, null]) {
      assert.throws(() => verifyEnvelope(JSON.stringify({ ...envelope, [field]: value }), trusted));
    }
  }
});

test('authenticated payload must be valid UTF-8 without replacing invalid bytes', () => {
  const envelope = envelopeFor(Buffer.from([0xc3, 0x28]));
  assert.throws(() => verifyEnvelope(JSON.stringify(envelope), trusted), /UTF-8/);
});

test('authentication returns bytes without granting schema or revision authorization', () => {
  const payload = Buffer.from('not JSON; schema and revision validation still required');
  assert.deepEqual(verifyEnvelope(JSON.stringify(envelopeFor(payload)), trusted), payload);
});

const now = '2026-10-24T12:00:00Z';
const hash = 'a'.repeat(64);
const document = () => ({
  schema: 1, revision: 1, issued_at: '2026-10-24T11:00:00Z', expires_at: '2026-10-25T11:00:00Z',
  minimum_app_version: '0.1.0', catalog: [], policies: [],
  drivers: [{ id: 'fixture', provider: 'claude', variant: 'stream-json' }],
  compatibility: [{ provider: 'claude', version: '1.2.3', operating_system: 'windows', architecture: 'x86_64',
    artifact_digest: hash, driver: 'fixture', outcome: 'qualified', reason: 'Fixture evidence',
    evidence: { startup: hash, turn: hash, cancellation: hash, cleanup: hash } }],
});
const parse = (value) => parsePayload(Buffer.from(JSON.stringify(value)), now, '0.2.0');

test('offline payload schema accepts only bounded, current, supported metadata', () => {
  assert.deepEqual(parse(document()), document());
  for (const mutate of [
    (d) => { d.schema = 2; }, (d) => { d.revision = 0; }, (d) => { d.revision = 1.5; },
    (d) => { d.issued_at = '2026-02-30T11:00:00Z'; }, (d) => { d.expires_at = now; },
    (d) => { d.minimum_app_version = '0.3.0'; }, (d) => { d.drivers[0].variant = 'downloaded-code'; },
    (d) => { d.drivers[0].command = 'run'; }, (d) => { d.compatibility[0].evidence.cleanup = null; },
    (d) => { d.compatibility.push(d.compatibility[0]); }, (d) => { d.permissions = 'full'; },
    (d) => { d.policies.push({ provider: 'codex', install: 'allow', launch: 'allow', reason: 'No suspension', evidence: hash }); },
  ]) { const d = document(); mutate(d); assert.throws(() => parse(d)); }
  const raw = JSON.stringify(document());
  assert.throws(() => parsePayload(Buffer.from(raw.replace('"revision":1', '"revision":2,"revision":1')), now, '0.2.0'));
  assert.throws(() => parsePayload(Buffer.alloc(MAX_ENVELOPE_BYTES + 1), now, '0.2.0'));
});


test('policy, identities, formats and size limits stay closed', () => {
  const valid = document();
  valid.policies.push({ provider: 'codex', install: 'suspend', launch: 'unchanged', reason: 'Qualification paused', evidence: hash });
  assert.deepEqual(parse(valid), valid);
  for (const mutate of [
    (d) => { d.catalog = [{ id: 'tool', commit: 'b'.repeat(40), digest: hash }, { id: 'tool', commit: 'b'.repeat(40), digest: hash }]; },
    (d) => { d.drivers.push(d.drivers[0]); }, (d) => { d.drivers[0].endpoint = 'https://example.org'; },
    (d) => { d.compatibility[0].model = 'chosen'; }, (d) => { d.compatibility[0].artifact_digest = null; },
    (d) => { d.compatibility[0].version = 'latest'; }, (d) => { d.compatibility[0].architecture = 'aarch64'; },
    (d) => { d.compatibility[0].evidence.startup = 'A'.repeat(64); },
    (d) => { d.policies.push({ provider: 'codex', install: 'unchanged', launch: 'unchanged', reason: 'Nothing', evidence: hash }); },
    (d) => { d.compatibility = Array(513).fill(d.compatibility[0]); },
    (d) => { d.minimum_app_version = '0.01.0'; },
  ]) { const d = document(); mutate(d); assert.throws(() => parse(d)); }
  const raw = JSON.stringify(document());
  assert.throws(() => parsePayload(Buffer.from(raw.replace('"startup":', '"start\\u0075p":null,"startup":')), now, '0.2.0'));
  assert.throws(() => parsePayload(Buffer.from([0xc3, 0x28]), now, '0.2.0'));
  assert.throws(() => parsePayload(Buffer.from(raw), now, '0.1.0-preview.1'));
});


test('one comma-containing field cannot replace all four evidence fields', () => {
  const d = document();
  d.compatibility[0].evidence = { 'cancellation,cleanup,startup,turn': hash };
  assert.throws(() => parse(d));
});
