// node --test scripts: a branch's fragments and placeholders get the numbers after phase-1-base's, and phase-1-base itself
// carries none.

import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import test from 'node:test';
import { FRAGMENTS, fold, numbered, placeholdersIn, plan, problems, quoted, SKIP, topDecision, topNote, twiceUsed } from './numbers.mjs';
import { body, entries } from './release-notes.mjs';

const decisions = '| R | Date |\n| :--- | :--- |\n| R160 | 2026-10-16 | A. |\n| R161 | 2026-10-16 | B. |\n';
const locale = "  'notes.14.new': 'A.',\n  \"notes.15.fixed\": \"B.\",\n";

test('the next numbers follow the highest decision and entry of the base', () => {
  assert.equal(topDecision(decisions), 161);
  assert.equal(topNote(locale), 15);
  assert.equal(topDecision(''), 0);
});

test('every placeholder of a branch is numbered, its second decision and entry after the first', () => {
  const row = '| R-next | 2026-10-17 | Amends R-next-2. |\n| R-next-2 | 2026-10-17 | C. |\n';
  const keys = "  'notes.next.new': 'X',\n  'notes.next.fixed': 'Y',\n  'notes.next-2.better': 'Z',\n";
  const code = '// The guard (R-next) keeps it.\n';
  const numbers = plan([row, keys, code], { decision: 161, note: 15 });
  assert.equal(numbered(row, numbers), '| R162 | 2026-10-17 | Amends R163. |\n| R163 | 2026-10-17 | C. |\n');
  assert.equal(numbered(keys, numbers), "  'notes.16.new': 'X',\n  'notes.16.fixed': 'Y',\n  'notes.17.better': 'Z',\n");
  assert.equal(numbered(code, numbers), '// The guard (R162) keeps it.\n');
});

test('quoted in backticks, as the docs that explain them do, the placeholders stay words', () => {
  const docs = 'Write `R-next` and `notes.next.<kind>` in a branch.\n';
  assert.deepEqual(placeholdersIn(docs), []);
  assert.equal(numbered(docs, plan([docs], { decision: 1, note: 1 })), docs);
  assert.deepEqual(placeholdersIn('a\n| R-next | x |\nb\n'), ['| R-next | x |']);
  assert.deepEqual(placeholdersIn("x:ui/locales/en.ts:9:  'notes.next.new': 'A',"), ["x:ui/locales/en.ts:9:  'notes.next.new': 'A',"]);
  assert.deepEqual(placeholdersIn('R-nextstep, SR-next, notes.nextgen'), []);
});

test('a decision number heading two rows is found', () => {
  assert.deepEqual(twiceUsed(decisions), []);
  assert.deepEqual(twiceUsed(`${decisions}| R161 | 2026-10-17 | C. |\n`), ['R161']);
});

test('docs/decisions.md uses each number once', () => {
  const root = join(import.meta.dirname, '..');
  assert.deepEqual(twiceUsed(readFileSync(join(root, 'docs/decisions.md'), 'utf8')), []);
});

const LANGS = ['en', 'pt-BR'];
const texts = (en, pt) => ({ en, 'pt-BR': pt });
const enLocale = "export const en = {\n  'notes.title': 'What’s new',\n  'notes.15.fixed': 'B.',\n  'other.key': 'X',\n} as const;\n";
const ptLocale = "export const ptBR: Messages = {\n  'notes.15.fixed': 'B pt.',\n  'other.key': 'X',\n};\n";
const tree = { locales: { en: enLocale, 'pt-BR': ptLocale }, decisions };

/** `tree` after `fragment` lands on it: folded, then numbered after its own highest numbers. */
function land(before, fragment) {
  const folded = fold([fragment], before);
  const numbers = plan([...Object.values(folded.locales), folded.decisions], { decision: topDecision(before.decisions), note: topNote(before.locales.en) });
  const locales = Object.fromEntries(Object.entries(folded.locales).map(([l, t]) => [l, numbered(t, numbers)]));
  return { locales, decisions: numbered(folded.decisions, numbers) };
}

test('a fragment needs every language of each kind it names, and rows written like the table', () => {
  assert.deepEqual(problems({ notes: { better: texts('Calmer.', 'Mais calmo.') }, decisions: ['| R-next | 2026-10-17 | A rule. | AI |'] }, LANGS), []);
  assert.deepEqual(problems({ decisions: ['| R-next-2 | 2026-10-17 | A rule. | Owner, details AI |'] }, LANGS), []);
  assert.deepEqual(problems({ notes: { better: { en: 'Only English.' } } }, LANGS), ['notes.better has no pt-BR text']);
  assert.deepEqual(problems({ notes: { great: texts('a', 'b') } }, LANGS), ['"great" is not a kind (new, better, fixed)']);
  assert.deepEqual(problems({ notes: { new: texts('a\nb', 'c') } }, LANGS), ['notes.new in en is more than one line']);
  assert.deepEqual(problems({ notes: { new: { ...texts('a', 'b'), xx: 'c' } } }, LANGS), ['notes.new has xx, which has no locale file']);
  assert.match(problems({ decisions: ['| R172 | 2026-10-17 | Numbered by hand. | AI |'] }, LANGS)[0], /^not a row/);
  assert.deepEqual(problems({ note: {} }, LANGS), ['"note" is not a field (only "notes" and "decisions" are)']);
  assert.deepEqual(problems({}, LANGS), ['it has no notes and no decisions']);
  assert.deepEqual(problems([], LANGS), ['it is not a JSON object']);
});

test('a fragment folds after the last entry of each locale and the last row, each kind an id of its own', () => {
  const fragment = { notes: { new: texts('It’s new.', 'É novo.'), fixed: texts("Don't stop.", 'Não para.') }, decisions: ['| R-next | 2026-10-17 | C. | AI |'] };
  const folded = fold([fragment], tree);
  assert.equal(
    folded.locales.en,
    "export const en = {\n  'notes.title': 'What’s new',\n  'notes.15.fixed': 'B.',\n  'notes.next.new': 'It’s new.',\n  'notes.next-2.fixed': \"Don't stop.\",\n  'other.key': 'X',\n} as const;\n",
  );
  assert.match(folded.locales['pt-BR'], /'notes\.15\.fixed': 'B pt\.',\n {2}'notes\.next\.new': 'É novo\.',\n {2}'notes\.next-2\.fixed': 'Não para\.',\n/);
  assert.equal(folded.decisions, `${decisions}| R-next | 2026-10-17 | C. | AI |\n`);
  const landed = land(tree, fragment);
  // The release body reads the folded entries as they were written (R142).
  assert.deepEqual(entries(landed.locales.en).map((e) => [e.id, e.kind, e.text]), [[15, 'fixed', 'B.'], [16, 'new', 'It’s new.'], [17, 'fixed', "Don't stop."]]);
  assert.equal(body(landed.locales.en, enLocale), "### New\n\n- It’s new.\n\n### Fixed\n\n- Don't stop.\n");
  assert.match(landed.decisions, /\| R161 \| 2026-10-16 \| B\. \|\n\| R162 \| 2026-10-17 \| C\. \| AI \|\n$/);
});

test('a fragment that already landed, carried again by a stacked branch, adds no entry and no row twice', () => {
  const fragment = { notes: { new: texts('It’s new.', 'É novo.'), fixed: texts("Don't stop.", 'Não para.') }, decisions: ['| R-next | 2026-10-17 | C. | AI |'] };
  const landed = land(tree, fragment);
  const again = fold([fragment, { notes: { better: texts('Other.', 'Outro.') } }], landed);
  const count = (text) => text.match(/^ {2}'notes\./gm).length;
  assert.equal(count(again.locales.en), count(landed.locales.en) + 1, 'only the new fragment adds an entry');
  assert.match(again.locales.en, /'notes\.next\.better': 'Other\.'/);
  assert.equal(again.decisions, landed.decisions);
});

test('a fragment beside an entry written straight into the locales takes the next placeholder id', () => {
  const old = enLocale.replace("'other.key'", "'notes.next.better': 'Old way.',\n  'other.key'");
  const folded = fold([{ notes: { new: texts('N.', 'N pt.') } }], { ...tree, locales: { en: old, 'pt-BR': ptLocale } });
  assert.match(folded.locales.en, /'notes\.next\.better': 'Old way\.',\n {2}'notes\.next-2\.new': 'N\.',/);
  assert.match(numbered(folded.locales.en, plan([folded.locales.en], { decision: 0, note: 15 })), /'notes\.16\.better'[^\n]*\n {2}'notes\.17\.new'/);
});

test('two branches taken from the same base land one after the other with different numbers', () => {
  const a = { notes: { better: texts('A.', 'A pt.') }, decisions: ['| R-next | 2026-10-17 | A. | AI |'] };
  const b = { notes: { fixed: texts('B2.', 'B2 pt.') }, decisions: ['| R-next | 2026-10-17 | B. | AI |'] };
  const both = land(land(tree, a), b);
  assert.deepEqual(entries(both.locales.en).map((e) => [e.id, e.text]), [[15, 'B.'], [16, 'A.'], [17, 'B2.']]);
  assert.deepEqual(twiceUsed(both.decisions), []);
  assert.equal(topDecision(both.decisions), 163);
  assert.deepEqual(placeholdersIn(Object.values(both.locales).join('\n') + both.decisions), []);
});

test('a text is quoted the way the locale files and the release body read it back', () => {
  assert.equal(quoted('plain'), "'plain'");
  assert.equal(quoted("it's"), `"it's"`);
  const hard = `say "hi", it's a \\ path`;
  assert.equal(entries(`  'notes.1.new': ${quoted(hard)},\n`)[0].text, hard);
});

test('every fragment waiting in docs/next folds into the real locales and decisions', () => {
  const root = join(import.meta.dirname, '..');
  const dir = join(root, FRAGMENTS);
  const names = existsSync(dir) ? readdirSync(dir).filter((n) => n.endsWith('.json')) : [];
  const langs = readdirSync(join(root, 'ui/locales')).filter((n) => n.endsWith('.ts')).map((n) => n.slice(0, -3));
  const locales = Object.fromEntries(langs.map((l) => [l, readFileSync(join(root, `ui/locales/${l}.ts`), 'utf8')]));
  const fragments = names.map((n) => JSON.parse(readFileSync(join(dir, n), 'utf8')));
  fragments.forEach((f, i) => assert.deepEqual(problems(f, langs), [], names[i]));
  assert.doesNotThrow(() => fold(fragments, { locales, decisions: readFileSync(join(root, 'docs/decisions.md'), 'utf8') }));
});

// Branches carry placeholders until `node scripts/numbers.mjs assign`; the shared branches never do.
const ref = process.env.GITHUB_REF ?? '';
const onBase = ref === 'refs/heads/phase-1-base' || ref === 'refs/heads/main';

test('phase-1-base carries no placeholder', { skip: !onBase && 'only on phase-1-base and main' }, () => {
  const root = join(import.meta.dirname, '..');
  const skip = new Set(['.git', 'node_modules', 'target', 'dist']);
  const found = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      const name = relative(root, path).replaceAll('\\', '/');
      if (entry.isDirectory()) {
        if (!skip.has(entry.name)) walk(path);
      } else if (/\.(md|ts|tsx|rs|mjs|js|toml|yml|json|css|html)$/.test(entry.name) && !SKIP.includes(name)) {
        for (const line of placeholdersIn(readFileSync(path, 'utf8'))) found.push(`${name}: ${line.trim()}`);
      }
    }
  };
  walk(root);
  assert.deepEqual(found, [], 'numbers are given when a change lands: node scripts/land.mjs <pull request>');
});
