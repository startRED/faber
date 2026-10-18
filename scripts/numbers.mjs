// Decision numbers (docs/decisions.md) and what's-new ids (`notes.<id>.<kind>` in ui/locales/*.ts) are given when a
// change lands, never in a branch: parallel lanes that each took "the next one" collided at every merge, and lanes that
// each appended to the same lines conflicted at every merge (docs/dev/workflow.md#numbers).
//
//   node scripts/numbers.mjs assign [--amend]   folds docs/next/*.json in, numbers every placeholder after a fresh
//                                               origin/phase-1-base, and commits; --amend puts it in the last commit and
//                                               numbers its message too (scripts/land.mjs runs this)
//   node scripts/numbers.mjs check [--base]     fails on a decision number used twice or a fragment that cannot fold;
//                                               with --base, on any placeholder or fragment left
//
// A branch puts its what's-new entry and its decision rows in a file of its own, docs/next/<branch>.json, so two branches
// never touch the same lines:
//
//   { "notes": { "better": { "en": "…", "pt-BR": "…", "es": "…", "fr": "…", "de": "…" } },
//     "decisions": ["| R-next | 2026-10-17 | The decision. | AI |"] }
//
// Elsewhere it writes R-next for its decision (R-next-2, R-next-3 for more), everywhere it refers to it. The older form,
// notes.next.<kind> written straight into the locales, is still numbered. Inside backticks the words are only quoted, as
// in the docs that explain them, and stay as they are.

import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const BASE = 'origin/phase-1-base';
// Not after a backtick (quoted) or a word character; the suffix tells a branch's second decision or entry from its first.
const DECISION = /(?<![`\w-])R-next(?:-(\d+))?(?![\w-])/g;
const NOTE = /(?<![`\w.-])notes\.next(?:-(\d+))?(?=\.)/g;
/** What `git grep -E` looks for first; `placeholdersIn` then drops the quoted ones. */
export const GREP = 'R-next|notes\\.next';
/** Numbers' own files, which spell the placeholders to find them. */
export const SKIP = ['scripts/numbers.mjs', 'scripts/numbers.test.mjs', 'scripts/land.mjs', 'scripts/land.test.mjs'];
/** Where a branch's entry and decision rows wait for their numbers, one file per branch. */
export const FRAGMENTS = 'docs/next';
const LOCALES = 'ui/locales';
const KINDS = ['new', 'better', 'fixed'];
// A row as docs/decisions.md writes it: number, date, decision, author.
const ROW = /^\| R-next(?:-\d+)? \| \d{4}-\d{2}-\d{2} \| [^\n]+ \| [^|\n]+ \|$/;
// An entry line of a locale file, numbered or not yet; the first group is its indent.
const ENTRY = /^(\s*)(['"])notes\.(?:\d+|next(?:-\d+)?)\.(?:new|better|fixed)\2:/;
// A row of the decisions table, numbered or not yet.
const TABLE_ROW = /^()\| R(?:\d|-next)/;

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

/** What keeps `fragment` (parsed JSON) from folding into the locales of the languages `langs`, one sentence each. */
export function problems(fragment, langs) {
  if (!fragment || typeof fragment !== 'object' || Array.isArray(fragment)) return ['it is not a JSON object'];
  const out = Object.keys(fragment)
    .filter((k) => k !== 'notes' && k !== 'decisions')
    .map((k) => `"${k}" is not a field (only "notes" and "decisions" are)`);
  const notes = fragment.notes ?? {};
  if (typeof notes !== 'object' || Array.isArray(notes)) out.push('"notes" maps a kind (new, better, fixed) to its texts');
  else {
    for (const [kind, texts] of Object.entries(notes)) {
      if (!KINDS.includes(kind)) {
        out.push(`"${kind}" is not a kind (new, better, fixed)`);
        continue;
      }
      for (const lang of langs) {
        const text = texts?.[lang];
        if (typeof text !== 'string' || !text.trim()) out.push(`notes.${kind} has no ${lang} text`);
        else if (/[\r\n]/.test(text)) out.push(`notes.${kind} in ${lang} is more than one line`);
      }
      for (const lang of Object.keys(texts ?? {})) if (!langs.includes(lang)) out.push(`notes.${kind} has ${lang}, which has no locale file`);
    }
  }
  const rows = fragment.decisions ?? [];
  if (!Array.isArray(rows)) out.push('"decisions" is a list of rows');
  else for (const row of rows) if (typeof row !== 'string' || !ROW.test(row)) out.push(`not a row "| R-next | YYYY-MM-DD | Decision. | Author |": ${row}`);
  if (out.length === 0 && Object.keys(notes).length === 0 && rows.length === 0) out.push('it has no notes and no decisions');
  return out;
}

/** `text` as a quoted value of a locale file: single quotes, or double ones when the text holds a single quote. */
export function quoted(text) {
  const plain = text.replaceAll('\\', '\\\\');
  return plain.includes("'") && !plain.includes('"') ? `"${plain}"` : `'${plain.replaceAll("'", "\\'")}'`;
}

/** `text` with `lines` after its last line that `pattern` matches, in that line's indent (the pattern's first group). */
function after(text, pattern, lines, what) {
  const all = text.split('\n');
  const at = all.findLastIndex((line) => pattern.test(line));
  if (at < 0) throw new Error(`there is no ${what} to add after`);
  const indent = pattern.exec(all[at])[1];
  all.splice(at + 1, 0, ...lines.map((line) => indent + line));
  return all.join('\n');
}

/**
 * The locales (`{ lang: text }`) and the text of docs/decisions.md with `fragments` folded in: each fragment's entries
 * after the last entry of every locale, each under a placeholder id of its own, and its rows at the end of the table. The
 * placeholders are numbered afterwards, with the rest of the branch's.
 */
export function fold(fragments, { locales, decisions }) {
  const taken = Object.values(locales).flatMap((t) => [...t.matchAll(NOTE)].map((m) => Number(m[1] ?? 1)));
  let suffix = Math.max(0, ...taken);
  const next = { locales: { ...locales }, decisions };
  // A fragment that already landed (a branch stacked on another carries its file) adds nothing twice: an entry whose
  // English text is an entry already, or a row the table holds under a number, is left out.
  const lines = (text) => (text ?? '').split('\n');
  const noted = (text) => lines(next.locales.en).some((line) => ENTRY.test(line) && line.trimEnd().endsWith(`: ${quoted(text)},`));
  const rest = (row) => row.replace(/^\| R(?:\d+|-next(?:-\d+)?) /, '');
  const decided = (row) => lines(next.decisions).some((line) => TABLE_ROW.test(line) && rest(line) === rest(row));
  for (const fragment of fragments) {
    const kinds = KINDS.filter((k) => fragment.notes?.[k] && !noted(fragment.notes[k].en));
    if (kinds.length) {
      // Each kind is an entry of its own: the screen and the release body use an id once (tests/ui/notes.check.ts).
      const ids = kinds.map(() => {
        suffix += 1;
        return suffix === 1 ? 'notes.next' : `notes.next-${suffix}`;
      });
      for (const lang of Object.keys(next.locales)) {
        const lines = kinds.map((k, i) => `${quoted(`${ids[i]}.${k}`)}: ${quoted(fragment.notes[k][lang])},`);
        next.locales[lang] = after(next.locales[lang], ENTRY, lines, `what's-new entry in ${lang}`);
      }
    }
    const rows = (fragment.decisions ?? []).filter((row) => !decided(row));
    if (rows.length) next.decisions = after(next.decisions, TABLE_ROW, rows, 'decision row');
  }
  return next;
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

/**
 * The fragments of the working tree, by file name: `{ file, fragment }`, or `{ file, broken }` when it is not JSON or
 * not a `.json` file right in docs/next (a branch named `a/b` writes `a-b.json`; anything else would never fold).
 */
function fragmentFiles() {
  if (!existsSync(FRAGMENTS)) return [];
  return readdirSync(FRAGMENTS, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((entry) => {
      const file = `${FRAGMENTS}/${entry.name}`;
      if (!entry.isFile() || !entry.name.endsWith('.json')) return { file, broken: 'only <name>.json files right in docs/next fold' };
      try {
        return { file, fragment: JSON.parse(readFileSync(file, 'utf8')) };
      } catch (error) {
        return { file, broken: `it is not JSON (${error.message})` };
      }
    });
}

/** The language of each locale file: its name without `.ts`. */
const languages = () => readdirSync(LOCALES).filter((n) => n.endsWith('.ts')).map((n) => n.slice(0, -3));

/** Folds `found` into the locales and the decisions of the working tree, and deletes them; `write` false only tries. */
function foldTree(found, write = true) {
  const langs = languages();
  const locales = Object.fromEntries(langs.map((l) => [l, readFileSync(`${LOCALES}/${l}.ts`, 'utf8')]));
  const decisions = readFileSync('docs/decisions.md', 'utf8');
  const next = fold(found.map((f) => f.fragment), { locales, decisions });
  if (!write) return;
  for (const l of langs) if (next.locales[l] !== locales[l]) writeFileSync(`${LOCALES}/${l}.ts`, next.locales[l]);
  if (next.decisions !== decisions) writeFileSync('docs/decisions.md', next.decisions);
  for (const { file } of found) rmSync(file);
}

/** Every reason the fragments of the working tree cannot fold, as `file: reason`. */
function fragmentProblems() {
  const langs = languages();
  const found = fragmentFiles();
  const out = found.flatMap(({ file, fragment, broken }) => (broken ? [broken] : problems(fragment, langs)).map((p) => `${file}: ${p}`));
  if (out.length) return out;
  try {
    foldTree(found, false);
    return [];
  } catch (error) {
    return [`${FRAGMENTS}: ${error.message}`];
  }
}

function assign(amend) {
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
  const broken = fragmentProblems();
  if (broken.length) {
    for (const p of broken) console.error(p);
    return 1;
  }
  const found = fragmentFiles();
  foldTree(found);
  // The tree holds the base, so its highest numbers are at least the base's; one written by hand in the branch counts too.
  const top = {
    decision: Math.max(topDecision(git('show', `${BASE}:docs/decisions.md`)), topDecision(readFileSync('docs/decisions.md', 'utf8'))),
    note: Math.max(topNote(git('show', `${BASE}:ui/locales/en.ts`)), topNote(readFileSync(`${LOCALES}/en.ts`, 'utf8'))),
  };
  const files = candidates();
  const texts = files.map((f) => readFileSync(f, 'utf8'));
  const message = amend ? git('log', '-1', '--format=%B') : '';
  const numbers = plan([...texts, message], top);
  if (numbers.decisions.size + numbers.notes.size === 0 && found.length === 0) {
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
  // Two rows that share a placeholder would share its number, and a change of words only runs no test that would see it.
  const twice = twiceUsed(readFileSync('docs/decisions.md', 'utf8'));
  if (twice.length) {
    git('reset', '--quiet', '--hard', 'HEAD');
    console.error(`${twice.join(', ')} would head two rows of docs/decisions.md: give each row its own placeholder (R-next, R-next-2).`);
    return 1;
  }
  git('add', '-A', '--', '.');
  if (amend) {
    execFileSync('git', ['commit', '--quiet', '--amend', '-F', '-'], { input: numbered(message, numbers), stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    console.log(`${said.join(', ') || 'Folded'}. Amended into ${git('rev-parse', '--short', 'HEAD').trim()}.`);
  } else {
    git('commit', '--quiet', '-m', `docs: number this branch's decisions and notes from phase-1-base\n\n${said.join('\n')}`);
    console.log(`${said.join(', ')}. Committed; push, and merge once CI is green.`);
  }
  return 0;
}

function check(base) {
  const problems = twiceUsed(readFileSync('docs/decisions.md', 'utf8')).map((r) => `${r} heads two rows of docs/decisions.md`);
  problems.push(...fragmentProblems());
  if (base) {
    for (const { file } of fragmentFiles()) problems.push(`${file} was never folded in`);
    for (const file of candidates()) {
      for (const line of placeholdersIn(readFileSync(file, 'utf8'))) problems.push(`${file}: ${line.trim()}`);
    }
  }
  for (const p of problems) console.error(p);
  if (problems.length && base) console.error('Numbers are given when a change lands: node scripts/land.mjs <pull request>');
  return problems.length ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(join(import.meta.dirname, 'numbers.mjs'))) {
  const [command, flag] = process.argv.slice(2);
  if (command === 'assign') process.exitCode = assign(flag === '--amend');
  else if (command === 'check') process.exitCode = check(flag === '--base');
  else {
    console.error('usage: node scripts/numbers.mjs assign [--amend] | check [--base]');
    process.exitCode = 2;
  }
}
