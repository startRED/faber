// Proof of "research-with-sources": node sources.mjs <folder> <pristine folder> <answer file>
// At most 20 distinct links are fetched; an answer with unchecked links cannot pass.
import { readFileSync } from 'node:fs';

const answer = readFileSync(process.argv[4], 'utf8');
const checks = [];
const check = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });

const urls = [...new Set([...answer.matchAll(/https?:\/\/[^\s)\]>"'`]+/g)].map((m) => m[0].replace(/[.,;:*_]+$/, '')))];
check('the answer cites at least four distinct links', urls.length >= 4, `${urls.length} links`);

async function reach(url) {
  const options = { redirect: 'follow', signal: AbortSignal.timeout(20_000), headers: { 'user-agent': 'Mozilla/5.0 (compatible; faber-bench-source-check)' } };
  try {
    const r = await fetch(url, options);
    // 401, 403 and 429 mean the page exists and keeps a script out; only a missing page or host is an invented source.
    return { url, status: r.status, ok: r.status < 400 || [401, 403, 429].includes(r.status) };
  } catch (error) {
    return { url, status: String(error.cause?.code ?? error.name), ok: false };
  }
}
const results = await Promise.all(urls.slice(0, 20).map(reach));
check('every cited link was checked', results.length === urls.length, `${results.length} of ${urls.length} links checked`);
const dead = results.filter((r) => !r.ok);
check('checked links open or return 401, 403 or 429', results.length > 0 && dead.length === 0, dead.map((r) => `${r.status} ${r.url}`).join('; '));
const placeholders = urls.filter((u) => /example\.(com|org)|yourdomain|placeholder/i.test(u));
check('no placeholder links', placeholders.length === 0, placeholders.join(', '));

console.log(JSON.stringify({ pass: checks.every((c) => c.ok), checks }));
