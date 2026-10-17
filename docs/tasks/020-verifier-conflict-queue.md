# A queue that knows conflicts (R190 V3)

Step V3 of `docs/superpowers/specs/2026-10-17-built-in-verifier.md`: changes that touch disjoint crates are checked
together, a failed batch is bisected, flaky tests are found from the verifier's journal, quarantined and rerun alone,
and `land.mjs`'s queue moves into the verifier. Keep R127's green base (no tests after merge).

Done when: two queued pull requests on disjoint crates land from one check; a failed batch names the culprit by
bisection; a test that flips on the same tree is marked flaky in the journal and no longer turns a batch red alone;
each part has an engine test with the fake process runner, and workflow.md describes the new queue.
