// Whether a change set is only words, so CI can skip the build and the suites (R163). The rule is testkit's
// (`suites_for` in testkit/src/plan.rs): a file under docs/ or ending in .md needs no suite, except the documents a test
// reads. Keep the two lists the same.
//
//   git diff --name-only A...B | node scripts/ci-scope.mjs    exits 0 when every file is only words, 3 when any is not
//
// An empty list is not "only words": nothing to compare means nothing proves the commit, so everything runs.

/** Documents a test reads: engine/tests/product_map.rs fails when the map and the code disagree. */
export const READ_BY_TESTS = ['docs/product-map.md'];

export const isWords = (file) => (file.startsWith('docs/') || file.endsWith('.md')) && !READ_BY_TESTS.includes(file);

/** The first file that is not only words, or null when every one is. An empty list answers with an empty string. */
export function firstCode(files) {
  if (files.length === 0) return '';
  return files.find((file) => !isWords(file)) ?? null;
}

if (process.argv[1]?.endsWith('ci-scope.mjs')) {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  const files = Buffer.concat(chunks).toString('utf8').split('\n').map((line) => line.trim()).filter(Boolean);
  const code = firstCode(files);
  if (code === null) {
    process.stdout.write(`Only words changed (${files.length} files).\n`);
  } else {
    process.stdout.write(code ? `${code} is not only words.\n` : 'No file changed.\n');
    process.exit(3);
  }
}
