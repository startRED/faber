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

/** The suites one path needs: the first rule that matches wins, and a path no rule matches needs every suite. */
export function suitesFor(file) {
  const path = file.replace(/^\.\//, '');
  const rule = rules.find(
    (r) =>
      (r.files ?? []).includes(path) ||
      (r.prefixes ?? []).some((p) => path.startsWith(p)) ||
      (r.suffixes ?? []).some((s) => path.endsWith(s)),
  );
  return rule ? rule.suites : SUITES;
}

/** Words need no suite and no lint: a document a test reads is code here, since its rule names a suite. */
export const isWords = (file) => suitesFor(file).length === 0;

/** Every decision for a change set. */
export function scope(files) {
  if (files.length === 0) return { lints: true, rust: true, ui: true, screen: true };
  const needed = new Set(files.flatMap(suitesFor));
  return { lints: !files.every(isWords), ...Object.fromEntries(SUITES.map((s) => [s, needed.has(s)])) };
}

if (process.argv[1]?.endsWith('ci-scope.mjs')) {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  const files = Buffer.concat(chunks).toString('utf8').split('\n').map((line) => line.trim()).filter(Boolean);
  for (const [name, needed] of Object.entries(scope(files))) process.stdout.write(`${name}=${needed}\n`);
}
