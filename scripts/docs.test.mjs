import { test } from 'node:test';
import assert from 'node:assert/strict';
import { anchors, links, mentions, problems, MAX_LINES } from './docs.mjs';

/** A tree of files for `problems`. */
function tree(files) {
  const has = (f) => f in files || Object.keys(files).some((k) => k.startsWith(`${f}/`));
  return problems(Object.keys(files), (f) => files[f], has);
}

const index = '# Docs\n\n| Page | Read it when |\n| :-- | :-- |\n| [a.md](a.md) | A |\n| [design/b.md](design/b.md) | B |\n';

test('a small, listed, correctly linked tree has no problem', () => {
  assert.deepEqual(
    tree({
      'docs/README.md': index,
      'docs/a.md': '# A\n\nSee [b](design/b.md#two-words).\n',
      'docs/design/b.md': '# B\n\n## Two words\n',
      'AGENTS.md': 'Read [a](docs/a.md).\n',
      'ui/x.ts': '// The look (docs/design/b.md#two-words).\n',
    }),
    [],
  );
});

test('a page past the limit fails until it splits, except the named large pages', () => {
  const long = '# A\n' + 'line\n'.repeat(MAX_LINES);
  const found = tree({ 'docs/README.md': index, 'docs/a.md': long, 'docs/design/b.md': '# B\n' });
  assert.equal(found.length, 1);
  assert.match(found[0], /^docs\/a\.md: \d+ lines/);
  assert.deepEqual(
    tree({ 'docs/README.md': `${index}| [decisions.md](decisions.md) | D |\n| [product-map.md](product-map.md) | M |\n| [security-r97-review.md](security-r97-review.md) | S |\n`, 'docs/a.md': '# A\n', 'docs/design/b.md': '# B\n', 'docs/decisions.md': long, 'docs/product-map.md': long, 'docs/security-r97-review.md': long }),
    [],
  );
});

test('a page missing from the index fails, history does not', () => {
  const found = tree({
    'docs/README.md': index,
    'docs/a.md': '# A\n',
    'docs/design/b.md': '# B\n',
    'docs/dev/c.md': '# C\n',
    'docs/research/old.md': '# Old\n'.repeat(500),
  });
  assert.deepEqual(found, ['docs/dev/c.md: not in docs/README.md; add a row that says when to read it']);
});

test('a link or a code pointer to a gone page or anchor fails', () => {
  const found = tree({
    'docs/README.md': index,
    'docs/a.md': '# A\n\n[gone](design.md#x) and [no anchor](design/b.md#nope).\n',
    'docs/design/b.md': '# B\n',
    'testkit/src/base.rs': '//! The red base (docs/design.md#red-base).\n',
    'engine/tests/t.rs': 'let sample = "docs/design.md#red-base";\n',
  });
  assert.deepEqual(found, [
    'docs/a.md: link design.md#x: docs/design.md does not exist',
    'docs/a.md: link design/b.md#nope: docs/design/b.md has no #nope',
    'testkit/src/base.rs: docs/design.md#red-base: docs/design.md does not exist',
  ]);
});

test('anchors follow GitHub, repeats included, and skip code blocks', () => {
  const a = anchors('# Faber design\n## Cache and router, measured on 13/10/2026\n## Notes\n## Notes\n```\n# not a heading\n```\n');
  assert.deepEqual([...a], ['faber-design', 'cache-and-router-measured-on-13102026', 'notes', 'notes-1']);
});

test('anchors come from the rendered text, and a repeat takes the first free suffix', () => {
  assert.deepEqual([...anchors('## _Helpful_ Section\n## `cargo t` and [map](m.md)\n## snake_case\n')], [
    'helpful-section',
    'cargo-t-and-map',
    'snake_case',
  ]);
  assert.deepEqual([...anchors('## A-1\n## A\n## A\n')], ['a-1', 'a', 'a-2']);
});

test('titled, angled, reference and root links are checked too', () => {
  assert.deepEqual(links('[a](gone.md "Title") [b](<c.md>) [e](/scripts/x.mjs)\n[ref]: other.md#part\n'), [
    'gone.md',
    'c.md',
    '/scripts/x.mjs',
    'other.md#part',
  ]);
  const found = tree({
    'docs/README.md': '| [a.md](a.md "A") | A |\n| [design/b.md](design/b.md) | B |\n',
    'docs/a.md': '# A\n\n[gone](gone.md "Title") and [root](/scripts/x.mjs).\n',
    'docs/design/b.md': '# B\n',
    'scripts/x.mjs': '',
  });
  assert.deepEqual(found, ['docs/a.md: link gone.md: docs/gone.md does not exist']);
});

test('links skip addresses, code and code blocks; mentions skip string literals', () => {
  assert.deepEqual(links('[a](b.md) [w](https://x.y) `[c](d.md)`\n```\n[e](f.md)\n```\n[g](#h)\n'), ['b.md', '#h']);
  assert.deepEqual(mentions('// see docs/dev/testing.md#screen-tests and "docs/x.md" or \'docs/y.md\''), ['docs/dev/testing.md#screen-tests']);
});
