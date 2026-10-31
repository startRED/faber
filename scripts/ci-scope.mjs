// Which suites a change set needs, so CI starts only those jobs (R163). The rules are testkit's own
// (testkit/suites.json, which `cargo t` reads too), so CI and a local run always pick the same suites.
//
//   git diff --name-only A...B | node scripts/ci-scope.mjs
//
// prints one line per decision for $GITHUB_OUTPUT: `lints=`, `rust=`, `ui=`, `screen=`, each `true` or `false`. The lints
// run for any file that is not only words. An empty list proves nothing, so everything runs.

import { readFileSync } from 'node:fs';

const { rules } = JSON.parse(readFileSync(new URL('../testkit/suites.json', import.meta.url), 'utf8'));

export const SUITES = ['rust', 'ui', 'screen'];

/** The first rule that matches a path, or none. */
function ruleFor(file) {
  const path = file.replace(/^\.\//, '');
  return rules.find(
    (r) =>
      (r.files ?? []).includes(path) ||
      (r.prefixes ?? []).some((p) => path.startsWith(p)) ||
      (r.suffixes ?? []).some((s) => path.endsWith(s)),
  );
}

/** The suites one path needs: the first rule that matches wins, and a path no rule matches needs every suite. */
export function suitesFor(file) {
  return ruleFor(file)?.suites ?? SUITES;
}

/** Words need no suite and no lint: a document a test reads is code here, since its rule names a suite, and so is a
 * script whose rule asks for the lints. */
export function isWords(file) {
  const rule = ruleFor(file);
  return rule !== undefined && rule.suites.length === 0 && !rule.lints;
}

/** Every decision for a change set. */
export function scope(files) {
  if (files.length === 0) return { lints: true, rust: true, ui: true, screen: true };
  const needed = new Set(files.flatMap(suitesFor));
  return { lints: !files.every(isWords), ...Object.fromEntries(SUITES.map((s) => [s, needed.has(s)])) };
}

/** The suites the PC runs before queueing a change to `files`: the ones CI would run, the lints aside (CI runs them). */
export function toCheck(files) {
  const needed = scope(files);
  return SUITES.filter((suite) => needed[suite]);
}

/**
 * The cheap checks the PC runs before queueing a change to `files`, so a lint error never costs a queue round: the format
 * check and Clippy for Rust and its manifests, oxlint for the screen's code, the type check for TypeScript, and always the
 * docs check, which also reads the page links in code. Checks only: CI runs its own lints anyway, and `faber/verify` never
 * names them. `test:scripts` stays out: its timing tests fail on a busy PC, and a lane runs it before each commit.
 */
export function lintsFor(files) {
  const rust = files.some((f) => /\.rs$|(^|\/)(Cargo\.(toml|lock)|clippy\.toml|rustfmt\.toml)$|^\.cargo\//.test(f));
  return [
    ...(rust ? ['fmt', 'clippy'] : []),
    ...(files.some((f) => /^(ui|tests\/ui)\/.*\.(tsx?|js)$/.test(f)) ? ['lint'] : []),
    ...(files.some((f) => /\.tsx?$|(^|\/)tsconfig[^/]*\.json$/.test(f)) ? ['typecheck'] : []),
    'docs',
  ];
}

/** The commit status the PC posts when its full check of a queued tree passed (R190 V2). */
export const STATUS = 'faber/verify';

/** What that status says: the very tree it checked and the suites that passed on it. */
export const statusText = (tree, suites) => `tree ${tree} covers ${suites.join(' ')}`;

/** The suites a `faber/verify` description covers for `tree`: none unless it names that very tree, in its exact words. */
export function covered(description, tree) {
  const found = /^tree ([0-9a-f]{40}) covers ((?:rust|ui|screen)(?: (?:rust|ui|screen))*)$/.exec(description ?? '');
  return found && found[1] === tree ? [...new Set(found[2].split(' '))] : [];
}

if (process.argv[1]?.endsWith('ci-scope.mjs')) {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  const files = Buffer.concat(chunks).toString('utf8').split('\n').map((line) => line.trim()).filter(Boolean);
  const decided = scope(files);
  // A green `faber/verify` of this very tree (FABER_VERIFY, its description; TREE, the tree of the commit) leaves out the
  // suites it covers. The lints always run.
  const done = covered(process.env.FABER_VERIFY, process.env.TREE);
  if (done.length) process.stderr.write(`${STATUS} covers ${done.join(' ')} on this tree: CI leaves them out\n`);
  for (const suite of done) decided[suite] = false;
  for (const [name, needed] of Object.entries(decided)) process.stdout.write(`${name}=${needed}\n`);
}
