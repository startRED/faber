// Offline signature boundary only: generated keys are disposable test fixtures, never account or production keys.
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import test from 'node:test';
import { MAX_ENVELOPE_BYTES, verifyEnvelope } from './live-manifest.mjs';

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
