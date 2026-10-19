// Keeps the living documents small enough to read whole, findable from docs/README.md, and linked correctly (R187).
// An agent reads only the pages its task names, so each page answers one kind of question; a page past the limit splits
// by topic instead of growing. Dated history (docs/superpowers/, docs/research/) and the logo sources are left alone.
//
//   node scripts/docs.mjs        (npm run docs)
//
// prints one line per problem and exits 1 when there is any.

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { posix } from 'node:path';

export const MAX_LINES = 240;
export const MAX_BYTES = 20_000;

/** Pages over the limit on purpose, and why. */
export const LARGE = {
  'docs/decisions.md': 'the ledger: one row per decision, searched by its words and read by scripts/numbers.mjs',
  'docs/product-map.md': 'read whole on purpose, to find a second way to do the same thing; engine/tests/product_map.rs reads it',
  'docs/security-r97-review.md': 'a verified review kept as written; only its R121/R124 section changes',
};

const HISTORY = ['docs/superpowers/', 'docs/research/', 'docs/brand/'];

/** A task of the queue: a brief that leaves with the pull request that does it, listed by docs/tasks/README.md. */
const isTask = (file) => file.startsWith('docs/tasks/') && file !== 'docs/tasks/README.md';

/** Living pages: every Markdown file under docs/ except history, the logo sources and the queue's tasks. */
export const isLiving = (file) =>
  file.startsWith('docs/') && file.endsWith('.md') && !HISTORY.some((h) => file.startsWith(h)) && !isTask(file);

/** A heading's anchor as GitHub writes it: from the rendered text, so links, code marks and emphasis go first. */
export const slug = (text) =>
  text
    .trim()
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/<[^>]+>/g, '')
    .replace(/[`*]/g, '')
    .replace(/(^|[^\p{L}\p{N}])_+|_+(?=[^\p{L}\p{N}]|$)/gu, '$1')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .replace(/\s/g, '-');

/** Every anchor of a Markdown text; a repeat gets the first free `-1`, `-2` as on GitHub. Code blocks hold no headings. */
export function anchors(text) {
  const taken = new Set();
  let fence = false;
  for (const line of text.split('\n')) {
    if (/^\s*(```|~~~)/.test(line)) fence = !fence;
    const heading = !fence && /^#{1,6} (.+)$/.exec(line);
    if (!heading) continue;
    const base = slug(heading[1].replace(/\s+#+\s*$/, ''));
    let anchor = base;
    for (let n = 1; taken.has(anchor); n++) anchor = `${base}-${n}`;
    taken.add(anchor);
  }
  return taken;
}

/** The relative links of a Markdown text, outside code: `[text](href)`, `[text](href "title")` and `[ref]: href`. */
export function links(text) {
  const out = [];
  let fence = false;
  for (const line of text.split('\n')) {
    if (/^\s*(```|~~~)/.test(line)) fence = !fence;
    if (fence) continue;
    const plain = line.replace(/`[^`]*`/g, '');
    const hrefs = [...plain.matchAll(/\]\(\s*<?([^)\s>]+)>?(?:\s+(?:"[^"]*"|'[^']*'))?\s*\)/g)].map((m) => m[1]);
    const ref = /^\s{0,3}\[[^\]]+\]:\s*<?([^\s>]+)/.exec(plain);
    if (ref) hrefs.push(ref[1]);
    for (const href of hrefs) if (!/^[a-z][a-z+.-]*:/i.test(href)) out.push(href);
  }
  return out;
}

/** `docs/…` paths named in source text, except in a string literal (a test's sample path, not a pointer). */
export function mentions(text) {
  return [...text.matchAll(/(?<!["'\w/.-])docs\/[\w./-]+?\.md(#[\w-]+)?/g)].map((m) => m[0]);
}

/** Every problem in the tree whose tracked files are `files`, read with `read`. */
export function problems(files, read, exists) {
  const found = [];
  const pages = files.filter(isLiving);
  const index = read('docs/README.md');
  const listed = new Set(links(index).map((href) => posix.join('docs', href.split('#')[0])));
  const anchorsOf = new Map();
  const anchorSet = (file) => {
    if (!anchorsOf.has(file)) anchorsOf.set(file, anchors(read(file)));
    return anchorsOf.get(file);
  };
  const resolve = (file, target, anchor, where) => {
    if (!exists(target)) return found.push(`${where}: ${target} does not exist`);
    if (anchor && target.endsWith('.md') && !anchorSet(target).has(anchor)) found.push(`${where}: ${target} has no #${anchor}`);
  };

  for (const page of pages) {
    const text = read(page);
    const lines = text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
    const bytes = Buffer.byteLength(text);
    if (!LARGE[page] && (lines > MAX_LINES || bytes > MAX_BYTES))
      found.push(`${page}: ${lines} lines, ${bytes} bytes; the limit is ${MAX_LINES} lines and ${MAX_BYTES} bytes: split it by topic`);
    if (page !== 'docs/README.md' && !listed.has(page)) found.push(`${page}: not in docs/README.md; add a row that says when to read it`);
  }

  // Links in the living pages and the agents' own guides.
  const guides = files.filter(
    (f) => isLiving(f) || f === 'AGENTS.md' || f === 'README.md' || /^\.claude\/skills\/.+\.md$/.test(f) || f === 'testkit/coherence/weekly.md',
  );
  for (const file of guides) {
    for (const href of links(read(file))) {
      const [path, anchor] = href.split('#');
      // A leading slash starts at the repository root, as on GitHub.
      const from = path.startsWith('/') ? path.slice(1) : posix.join(posix.dirname(file), path);
      const target = path ? posix.normalize(from).replace(/\/$/, '') : file;
      resolve(file, target, anchor, `${file}: link ${href}`);
    }
  }

  // Pointers to living pages from code, comments and config.
  for (const file of files) {
    // A script test's sample paths are status lines, not pointers.
    if (file.startsWith('docs/') || /\.test\.mjs$/.test(file)) continue;
    if (!/\.(rs|ts|tsx|mjs|js|css|ya?ml|json|toml|md)$|^\.gitignore$/.test(file)) continue;
    for (const mention of mentions(read(file))) {
      const [target, anchor] = mention.split('#');
      if (HISTORY.some((h) => target.startsWith(h))) continue;
      resolve(file, target, anchor, `${file}: ${mention}`);
    }
  }
  return found;
}

if (process.argv[1]?.endsWith('docs.mjs')) {
  const files = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard'], { encoding: 'utf8' })
    .split('\n')
    .filter((f) => f && existsSync(f));
  const read = (f) => readFileSync(f, 'utf8').replace(/\r\n/g, '\n');
  const exists = (f) => existsSync(f) && (statSync(f).isFile() || statSync(f).isDirectory());
  const found = problems(files, read, exists);
  for (const line of found) console.log(line);
  if (found.length) process.exit(1);
  console.log(`docs: ${files.filter(isLiving).length} living pages, each within its limit, listed and linked`);
}
