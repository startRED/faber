// Offline publication boundary. A verified signature is not schema, freshness or installation authorization.
import { KeyObject, verify } from 'node:crypto';
import { isUtf8 } from 'node:buffer';

export const MAX_ENVELOPE_BYTES = 1024 * 1024;

function decode(value) {
  if (typeof value !== 'string' || !value.length || value.length % 4 !== 0
    || !/^[A-Za-z0-9+/]*={0,2}$/.test(value)) throw new Error('Invalid manifest base64');
  const bytes = Buffer.from(value, 'base64');
  if (bytes.toString('base64') !== value) throw new Error('Invalid manifest base64');
  return bytes;
}

/** Return the exact authenticated bytes; callers must validate their schema and authorization separately. */
export function verifyEnvelope(text, trustedKeys) {
  if (typeof text !== 'string' || Buffer.byteLength(text, 'utf8') > MAX_ENVELOPE_BYTES) {
    throw new Error('Invalid manifest envelope size');
  }
  const envelope = JSON.parse(text);
  if (!envelope || Array.isArray(envelope) || typeof envelope !== 'object'
    || Object.keys(envelope).sort().join(',') !== 'key_id,payload,signature'
    || typeof envelope.key_id !== 'string' || !/^[a-z0-9][a-z0-9-]{0,63}$/.test(envelope.key_id)) {
    throw new Error('Invalid manifest envelope');
  }
  const key = trustedKeys.get(envelope.key_id);
  if (!(key instanceof KeyObject) || key.type !== 'public' || key.asymmetricKeyType !== 'ed25519') {
    throw new Error('Unknown manifest signing key');
  }
  const payload = decode(envelope.payload);
  const signature = decode(envelope.signature);
  if (signature.length !== 64 || !verify(null, payload, key, signature)) {
    throw new Error('Invalid manifest signature');
  }
  if (!isUtf8(payload)) throw new Error('Invalid manifest UTF-8');
  return payload;
}
