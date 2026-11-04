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

// This validates authenticated bytes only; revision persistence and installation authority are separate.
export function parsePayload(bytes, now, appVersion) {
  const fail = () => { throw new Error('Invalid manifest payload'); };
  if (!Buffer.isBuffer(bytes) || bytes.length > MAX_ENVELOPE_BYTES || !isUtf8(bytes)) fail();
  const text = bytes.toString('utf8');
  const data = JSON.parse(text);
  // JSON.parse discards duplicate names, so inspect names before trusting the parsed object.
  const stack = [];
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') {
      const start = i++;
      while (i < text.length && text[i] !== '"') { if (text[i] === '\\') i++; i++; }
      const name = JSON.parse(text.slice(start, i + 1));
      let next = i + 1;
      while (/\s/.test(text[next] ?? '') && next < text.length) next++;
      if (text[next] === ':' && stack.at(-1) instanceof Set) {
        if (stack.at(-1).has(name)) fail();
        stack.at(-1).add(name);
      }
    } else if (c === '{' || c === '[') {
      stack.push(c === '{' ? new Set() : null);
      if (stack.length > 8) fail();
    } else if (c === '}' || c === ']') stack.pop();
  }
  const object = (value, keys) => value && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).length === keys.split(' ').length
    && keys.split(' ').every((key) => Object.hasOwn(value, key));
  const id = (v) => typeof v === 'string' && /^[a-z0-9][a-z0-9-]{0,63}$/.test(v);
  const digest = (v) => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
  const reason = (v) => typeof v === 'string' && [...v].length > 0 && [...v].length <= 512 && !/[\u0000-\u001f\u007f-\u009f]/u.test(v);
  const date = (v) => {
    if (typeof v !== 'string' || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/.test(v)) fail();
    const n = Date.parse(v);
    if (!Number.isFinite(n) || new Date(n).toISOString() !== v.replace('Z', '.000Z')) fail();
    return n;
  };
  const version = (v, stable = false) => {
    if (typeof v !== 'string' || v.length > 128) fail();
    const m = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/.exec(v);
    if (!m || m.slice(1, 4).some((n) => Number(n) > 4294967295)
      || (m[4] && m[4].split('.').some((p) => /^0\d+$/.test(p))) || (stable && (m[4] || m[5]))) fail();
    return m;
  };
  if (!object(data, 'schema revision issued_at expires_at minimum_app_version catalog compatibility policies drivers')
    || data.schema !== 1 || !Number.isSafeInteger(data.revision) || data.revision < 1) fail();
  const issued = date(data.issued_at); const expires = date(data.expires_at); const current = date(now);
  if (issued > current || expires <= current || expires <= issued) fail();
  const min = version(data.minimum_app_version, true); const app = version(appVersion);
  const comparison = [1, 2, 3].map((i) => Number(app[i]) - Number(min[i])).find((n) => n !== 0) ?? 0;
  if (comparison < 0 || (comparison === 0 && app[4])) fail();
  for (const [list, max] of [[data.catalog, 512], [data.compatibility, 512], [data.policies, 2], [data.drivers, 64]]) {
    if (!Array.isArray(list) || list.length > max) fail();
  }
  const unique = (set, value) => { if (set.has(value)) fail(); set.add(value); };
  const providers = new Map([['claude', 'stream-json'], ['codex', 'app-server']]);
  const catalog = new Set();
  for (const entry of data.catalog) {
    if (!object(entry, 'id commit digest') || !id(entry.id) || typeof entry.commit !== 'string' || !/^[a-f0-9]{40}$/.test(entry.commit) || !digest(entry.digest)) fail();
    unique(catalog, entry.id);
  }
  const policies = new Set();
  for (const entry of data.policies) {
    if (!object(entry, 'provider install launch reason evidence') || !providers.has(entry.provider)
      || !['suspend', 'unchanged'].includes(entry.install) || !['suspend', 'unchanged'].includes(entry.launch)
      || (entry.install !== 'suspend' && entry.launch !== 'suspend') || !reason(entry.reason) || !digest(entry.evidence)) fail();
    unique(policies, entry.provider);
  }
  const drivers = new Map();
  for (const entry of data.drivers) {
    if (!object(entry, 'id provider variant') || !id(entry.id) || providers.get(entry.provider) !== entry.variant || drivers.has(entry.id)) fail();
    drivers.set(entry.id, entry.provider);
  }
  const records = new Set(); const used = new Set();
  for (const entry of data.compatibility) {
    if (!object(entry, 'provider version operating_system architecture artifact_digest driver outcome evidence reason')
      || !providers.has(entry.provider) || drivers.get(entry.driver) !== entry.provider
      || !['windows', 'macos', 'linux'].includes(entry.operating_system)
      || !['x86_64', 'aarch64'].includes(entry.architecture)
      || (entry.operating_system === 'windows' && entry.architecture !== 'x86_64')
      || !['qualified', 'unqualified'].includes(entry.outcome) || !reason(entry.reason)
      || !object(entry.evidence, 'startup turn cancellation cleanup')) fail();
    version(entry.version);
    for (const value of [entry.artifact_digest, ...Object.values(entry.evidence)]) {
      if (value === null ? entry.outcome === 'qualified' : !digest(value)) fail();
    }
    unique(records, JSON.stringify([entry.provider, entry.version, entry.operating_system, entry.architecture]));
    used.add(entry.driver);
  }
  if (used.size !== drivers.size) fail();
  return data;
}
