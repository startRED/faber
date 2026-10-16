// Decision numbers (docs/decisions.md) and what's-new ids (`notes.<id>.<kind>` in ui/locales/*.ts) are given at merge
// time, never in a branch: parallel lanes that each took "the next one" collided at every merge (docs/design.md#numbers).
//
//   node scripts/numbers.mjs assign   numbers this branch's placeholders from a fresh origin/phase-1-base and commits
//   node scripts/numbers.mjs check    fails on a decision number used twice; with --base, on any placeholder left
//
// A branch writes R-next for its decision (R-next-2, R-next-3 for more) and notes.next.<kind> for its entry (notes.next-2
// for a second id), everywhere it refers to them. Inside backticks the words are only quoted, as in the docs that
// explain them, and stay as they are.

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const BASE = 'origin/phase-1-base';
// Not after a backtick (quoted) or a word character; the suffix tells a branch's second decision or entry from its first.
const DECISION = /(?<![`\w-])R-next(?:-(\d+))?(?![\w-])/g;
const NOTE = /(?<![`\w.-])notes\.next(?:-(\d+))?(?=\.)/g;
/** What `git grep -E` looks for first; `placeholdersIn` then drops the quoted ones. */
export const GREP = 'R-next|notes\\.next';
/** Numbers' own files, which spell the placeholders to find them. */
export const SKIP = ['scripts/numbers.mjs', 'scripts/numbers.test.mjs'];

/** The highest decision number in the text of docs/decisions.md. */
export const topDecision = (text) => Math.max(0, ...[...text.matchAll(/^\| R(\d+)\b/gm)].map((m) => Number(m[1])));

/** The highest what's-new id in the text of a locale file. */
export const topNote = (text) => Math.max(0, ...[...text.matchAll(/['"]notes\.(\d+)\./g)].map((m) => Number(m[1])));

/** Decision numbers that head more than one row of docs/decisions.md. */
export function twiceUsed(text) {
  const seen = new Set();
  const twice = new Set();
  for (const [, n] of text.matchAll(/^\| R(\d+) \|/gm)) (seen.has(n) ? twice : seen).add(n);
  return [...twice].map((n) => `R${n}`);
}

/** The lines of `text` (or of `git grep -n` output) that hold a placeholder. */
export const placeholdersIn = (text) => text.split('\n').filter((line) => new RegExp(DECISION).test(line) || new RegExp(NOTE).test(line));

/**
 * `{ decisions, notes }`: each placeholder found in `texts` mapped to its number, the first after `top.decision` and
 * `top.note`, in suffix order (no suffix first).
 */
export function plan(texts, top) {
  const order = (pattern) => {
    const suffixes = new Set(texts.flatMap((t) => [...t.matchAll(pattern)].map((m) => Number(m[1] ?? 1))));
    return [...suffixes].sort((a, b) => a - b);
  };
  const decisions = new Map(order(DECISION).map((s, i) => [s, top.decision + i + 1]));
  const notes = new Map(order(NOTE).map((s, i) => [s, top.note + i + 1]));
  return { decisions, notes };
}

/** `text` with every placeholder replaced by its number from `plan`. */
export function numbered(text, { decisions, notes }) {
  return text
    .replace(DECISION, (_, s) => `R${decisions.get(Number(s ?? 1))}`)
    .replace(NOTE, (_, s) => `notes.${notes.get(Number(s ?? 1))}`);
}

const git = (...args) => execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });

/** The tracked files of the working tree that mention a placeholder, numbers' own files left out. */
function candidates() {
  try {
    return git('grep', '-I', '-l', '-E', GREP, '--', '.', ...SKIP.map((f) => `:!${f}`)).split('\n').filter(Boolean);
  } catch {
    return []; // `git grep` exits 1 when nothing matches.
  }
}

function assign() {
  if (git('status', '--porcelain').trim()) {
    console.error('Commit or set aside your changes first: the numbers go in a commit of their own.');
    return 1;
  }
  git('fetch', '--quiet', 'origin', 'phase-1-base');
  try {
    git('merge-base', '--is-ancestor', BASE, 'HEAD');
  } catch {
    console.error(`This branch is behind ${BASE}. Bring it in first (git merge ${BASE}), then run this again.`);
    return 1;
  }
  const top = { decision: topDecision(git('show', `${BASE}:docs/decisions.md`)), note: topNote(git('show', `${BASE}:ui/locales/en.ts`)) };
  const files = candidates();
  const texts = files.map((f) => readFileSync(f, 'utf8'));
  const numbers = plan(texts, top);
  if (numbers.decisions.size + numbers.notes.size === 0) {
    console.log('Nothing to number.');
    return 0;
  }
  files.forEach((file, i) => {
    const next = numbered(texts[i], numbers);
    if (next !== texts[i]) writeFileSync(file, next);
  });
  const said = [
    ...[...numbers.decisions].map(([s, n]) => `${s === 1 ? 'R-next' : `R-next-${s}`} is R${n}`),
    ...[...numbers.notes].map(([s, n]) => `${s === 1 ? 'notes.next' : `notes.next-${s}`} is notes.${n}`),
  ];
  git('commit', '--quiet', '-a', '-m', `docs: number this branch's decisions and notes from phase-1-base\n\n${said.join('\n')}`);
  console.log(`${said.join(', ')}. Committed; push, and merge once CI is green.`);
  return 0;
}

function check(base) {
  const problems = twiceUsed(readFileSync('docs/decisions.md', 'utf8')).map((r) => `${r} heads two rows of docs/decisions.md`);
  if (base) {
    for (const file of candidates()) {
      for (const line of placeholdersIn(readFileSync(file, 'utf8'))) problems.push(`${file}: ${line.trim()}`);
    }
  }
  for (const p of problems) console.error(p);
  if (problems.length && base) console.error('Placeholders are numbered at merge time: node scripts/numbers.mjs assign');
  return problems.length ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'numbers.mjs'))) {
  const [command, flag] = process.argv.slice(2);
  if (command === 'assign') process.exitCode = assign();
  else if (command === 'check') process.exitCode = check(flag === '--base');
  else {
    console.error('usage: node scripts/numbers.mjs assign | check [--base]');
    process.exitCode = 2;
  }
}
