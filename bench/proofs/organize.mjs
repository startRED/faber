// Proof of "organize-folder": node organize.mjs <final folder> <pristine folder> <answer file>
// Prints one JSON line: {"pass":bool,"checks":[{"name","ok","detail"}]}. Dot files and folders (.git, .faber) are not the work.
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const [folder, pristine] = process.argv.slice(2);
const checks = [];
const check = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name.startsWith('.')) continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
}
const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');

const before = walk(pristine).map((p) => ({ name: relative(pristine, p), hash: hash(p) }));
const after = walk(folder);
const afterHashes = new Map();
for (const p of after) afterHashes.set(hash(p), (afterHashes.get(hash(p)) ?? 0) + 1);

const lost = before.filter((f) => !afterHashes.has(f.hash)).map((f) => f.name);
check('every original file is still there, content unchanged', lost.length === 0, lost.length ? `missing: ${lost.slice(0, 6).join(', ')}` : `${before.length} files`);
const copies = before.filter((f) => afterHashes.get(f.hash) > 1).map((f) => f.name);
check('no file was copied instead of moved', copies.length === 0, copies.slice(0, 6).join(', '));

const top = readdirSync(folder).filter((n) => !n.startsWith('.'));
const topFiles = top.filter((n) => statSync(join(folder, n)).isFile());
const topDirs = top.filter((n) => statSync(join(folder, n)).isDirectory());
check('only INDEX.md is left loose in the top folder', topFiles.every((n) => n.toLowerCase() === 'index.md'), `loose: ${topFiles.join(', ')}`);
check('between 3 and 10 folders', topDirs.length >= 3 && topDirs.length <= 10, `${topDirs.length}: ${topDirs.join(', ')}`);
const sizes = topDirs.map((d) => walk(join(folder, d)).length);
check('no folder is empty', sizes.every((n) => n > 0), topDirs.filter((_, i) => sizes[i] === 0).join(', '));
check('no folder holds most of the files', Math.max(0, ...sizes) <= Math.ceil(before.length * 0.6), `largest ${Math.max(0, ...sizes)} of ${before.length}`);

const index = join(folder, 'INDEX.md');
const text = existsSync(index) ? readFileSync(index, 'utf8').toLowerCase() : '';
check('INDEX.md exists', text.length > 0);
const unnamed = topDirs.filter((d) => !text.includes(d.toLowerCase()));
check('INDEX.md names every folder', text.length > 0 && unnamed.length === 0, unnamed.join(', '));

console.log(JSON.stringify({ pass: checks.every((c) => c.ok), checks }));
